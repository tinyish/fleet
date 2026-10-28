//! Where a command's words go, and the ink they are written in. Each line
//! carries a mark: on a terminal the mark is a coloured symbol at the start
//! of the line; piped, there is no symbol: warnings and errors start with
//! `fleet:` instead, and hints with `hint:`. Colour, like the symbol, shows only on a terminal, so a
//! script reads plain text.

use std::fmt::Display;
use std::io::{self, IsTerminal, Write};
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use crate::cmd::doctor::{Found, Verdict};
use crate::protocol::{Response, Status};

use crate::ui::theme::{ERROR, NEXT, SUCCESS, Tone, WARN, paint_on};
use crate::ui::words;

/// The ink for one stream: colour and the length of paths are decided by
/// whether that stream is a terminal, asked of the stream itself unless the
/// ink says.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ink {
    stderr: bool,
    terminal: Option<bool>,
}

/// Ink for done lines, on stdout.
pub(crate) const OUT: Ink = Ink {
    stderr: false,
    terminal: None,
};

/// Ink for refusals and what could not be told, on stderr.
pub(crate) const ERR: Ink = Ink {
    stderr: true,
    terminal: None,
};

impl Ink {
    fn tone(self, tone: Tone, text: impl Display) -> String {
        paint_on(tone, self.stderr).apply_to(text).to_string()
    }

    /// What to notice: a path, a name, a command.
    pub(crate) fn accent(self, text: impl Display) -> String {
        self.tone(Tone::Accent, text)
    }

    /// What is secondary: an event's id and time.
    pub(crate) fn muted(self, text: impl Display) -> String {
        self.tone(Tone::Muted, text)
    }

    /// One path, as something to notice: on a terminal only its last two
    /// directories, so a line stays readable; piped, whole, for a script.
    pub(crate) fn path(self, path: &Path) -> String {
        if self.terminal() {
            self.accent(last_two(path))
        } else {
            self.whole_path(path)
        }
    }

    /// One path, whole even on a terminal: one a hint asks a person to type.
    pub(crate) fn whole_path(self, path: &Path) -> String {
        self.accent(path.display())
    }

    /// This ink as a script sees it, whatever the stream: for tests, so the
    /// words read the same whether or not `cargo test` runs at a terminal.
    #[cfg(test)]
    pub(crate) const fn piped(self) -> Ink {
        Ink {
            terminal: Some(false),
            ..self
        }
    }

    fn terminal(self) -> bool {
        if let Some(terminal) = self.terminal {
            return terminal;
        }
        if self.stderr {
            io::stderr().is_terminal()
        } else {
            io::stdout().is_terminal()
        }
    }

    /// Several paths, each to notice, separated by commas.
    pub(crate) fn paths(self, paths: &[PathBuf]) -> String {
        paths
            .iter()
            .map(|p| self.path(p))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// The last two directories of `path` (`code/app` for `/Users/you/code/app`),
/// or `path` as it is when it has two or fewer.
fn last_two(path: &Path) -> String {
    let names: Vec<&std::ffi::OsStr> = path
        .components()
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name),
            _ => None,
        })
        .collect();
    match names.as_slice() {
        [_, .., parent, name] => Path::new(parent).join(name).display().to_string(),
        _ => path.display().to_string(),
    }
}

/// What a line is, shown by the symbol it starts with on a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mark {
    /// What was done: `◆`, on stdout.
    Done,
    /// Something to look at, or that could not be told: `▲`, on stderr.
    Warn,
    /// What was refused or failed: `■`, on stderr.
    Error,
    /// What to do next, under the line it follows: `>`, on stderr.
    Next,
    /// What a command is doing while it waits: no symbol, on stderr.
    Plain,
}

impl Mark {
    fn on_stderr(self) -> bool {
        !matches!(self, Mark::Done)
    }

    /// What starts the line in place of the symbol when it is piped.
    fn piped_prefix(self) -> &'static str {
        match self {
            Mark::Done | Mark::Plain => "",
            Mark::Warn | Mark::Error => "fleet: ",
            Mark::Next => "hint: ",
        }
    }

    /// The symbol a line starts with on a terminal, and its tone; none for
    /// a plain line.
    fn symbol(self) -> Option<(&'static str, Tone)> {
        match self {
            Mark::Done => Some((SUCCESS, Tone::Good)),
            Mark::Warn => Some((WARN, Tone::Warn)),
            Mark::Error => Some((ERROR, Tone::Bad)),
            Mark::Next => Some((NEXT, Tone::Accent)),
            Mark::Plain => None,
        }
    }
}

/// The line `text` as `mark` shows it on a stream that `terminal` or not:
/// the coloured symbol on a terminal; piped, `fleet:` before a warning or
/// error, `hint:` before a hint, nothing before a done line. Lines after the first are
/// indented under the text on a terminal. A plain line is its text alone.
fn shown(mark: Mark, text: &str, terminal: bool) -> String {
    if terminal && let Some((symbol, tone)) = mark.symbol() {
        let symbol = paint_on(tone, mark.on_stderr()).apply_to(symbol);
        let mut shown = format!("{symbol} ");
        for (i, line) in text.lines().enumerate() {
            if i > 0 {
                shown.push_str("\n  ");
            }
            shown.push_str(line);
        }
        shown
    } else {
        format!("{}{text}", mark.piped_prefix())
    }
}

/// Prints one line, with its mark, on the stream the mark belongs to. A
/// reader that closed the pipe early is not an error.
pub(crate) fn line(mark: Mark, text: &str) {
    let _ = if mark.on_stderr() {
        let terminal = io::stderr().is_terminal();
        writeln!(io::stderr(), "{}", shown(mark, text, terminal))
    } else {
        let terminal = io::stdout().is_terminal();
        writeln!(io::stdout(), "{}", shown(mark, text, terminal))
    };
}

/// The fleets a start forgot because their directory is gone.
fn forgotten(ending: &Result<Response, Status>) -> &[PathBuf] {
    match ending {
        Ok(Response::Started { response, .. }) | Err(Status::PartlyOn { response, .. }) => {
            &response.forgotten
        }
        Ok(Response::ProjectOn { response, .. }) => &response.forgotten,
        _ => &[],
    }
}

/// The lines an ending prints, each with its mark, then what to do next when
/// there is something to do. Most endings are one line; a start that left
/// some fleets off is a done line, then one error line per fleet. A start
/// that forgot fleets says so, one warning line each, after its done line.
pub(crate) fn lines(ending: &Result<Response, Status>) -> Vec<(Mark, String)> {
    let gone = forgotten(ending)
        .iter()
        .map(|dir| (Mark::Warn, words::remove::forgotten(dir, ERR)));
    let (mut lines, next) = match ending {
        // Events said everything as it went.
        Ok(Response::Followed) => return vec![],
        Ok(response) => (
            std::iter::once((Mark::Done, words::response(response, OUT)))
                .chain(gone)
                .collect(),
            words::next_after(response, ERR),
        ),
        Err(status @ Status::PartlyOn { response, startup }) => {
            let mut lines: Vec<(Mark, String)> =
                std::iter::once((Mark::Done, words::partly_on(response, *startup, OUT)))
                    .chain(gone)
                    .collect();
            for off in &response.stayed_off {
                lines.push((Mark::Error, words::stayed_off(off, ERR)));
            }
            (lines, words::next(status, ERR))
        }
        Err(status) => {
            let mark = match status {
                Status::CouldNotTell(_) | Status::TimedOut { .. } => Mark::Warn,
                _ => Mark::Error,
            };
            (
                vec![(mark, words::status(status, ERR))],
                words::next(status, ERR),
            )
        }
    };
    if let Some(next) = next {
        lines.push((Mark::Next, next));
    }
    lines
}

/// Prints how a command ended and returns its exit code.
pub(crate) fn report(ending: &Result<Response, Status>) -> ExitCode {
    for (mark, text) in lines(ending) {
        line(mark, &text);
    }
    match ending {
        Ok(_) => ExitCode::SUCCESS,
        Err(status) => ExitCode::from(status.exit_code()),
    }
}

/// Prints doctor's report on stdout.
pub(crate) fn doctor(report: &[Found]) {
    let mut out = io::stdout();
    for line in doctor_lines(report, io::stdout().is_terminal()) {
        let _ = writeln!(out, "{line}");
    }
}

/// Doctor's report as lines: one per check, then what to do about any that
/// has something to do, each hint once. On a `terminal` a row is a bar in
/// the verdict's colour, the check in bold, then `healthy` and what was
/// found in gray, or, for anything else, what was found in the verdict's
/// colour. Piped, a row starts with the verdict in words so a script can
/// read the column.
fn doctor_lines(report: &[Found], terminal: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut hints: Vec<String> = Vec::new();
    for found in report {
        let name = words::checked(found.checked());
        let said = words::found(found, OUT);
        let verdict = found.verdict();
        if terminal {
            // One colour for the whole detail: a path's own accent would
            // break the row's colour where it ends.
            let said = console::strip_ansi_codes(&said);
            let tone = match verdict {
                Verdict::Fine => Tone::Good,
                Verdict::Warn | Verdict::CouldNotTell => Tone::Warn,
                Verdict::Missing => Tone::Bad,
            };
            let bar = paint_on(tone, false).apply_to("┃");
            let name = console::style(format!("{name:<30}")).for_stdout().bold();
            let state = if verdict == Verdict::Fine {
                let healthy = console::style(words::HEALTHY).for_stdout().bold();
                format!("{healthy} {}", paint_on(Tone::Muted, false).apply_to(said))
            } else {
                paint_on(tone, false).apply_to(said).to_string()
            };
            lines.push(format!("{bar} {name}{state}"));
        } else {
            lines.push(format!("{:<15} {name:<16} {said}", words::verdict(verdict)));
        }
        if let Some(next) = words::found_next(found, OUT)
            && !hints.contains(&next)
        {
            hints.push(next);
        }
    }
    if !hints.is_empty() {
        lines.push(String::new());
    }
    for next in hints {
        lines.push(if terminal {
            format!("{} {next}", paint_on(Tone::Accent, false).apply_to(NEXT))
        } else {
            format!("{}{next}", Mark::Next.piped_prefix())
        });
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_core::{protocol, status};

    #[test]
    fn a_path_keeps_its_last_two_directories_or_shows_as_it_is() {
        assert_eq!(last_two(Path::new("/Users/you/code/app")), "code/app");
        assert_eq!(
            last_two(Path::new("/Users/you/Library/Application Support/fleet")),
            "Application Support/fleet"
        );
        assert_eq!(last_two(Path::new("/code/app")), "/code/app");
        assert_eq!(last_two(Path::new("/app")), "/app");
        assert_eq!(last_two(Path::new("/")), "/");
    }

    #[test]
    fn piped_lines_have_no_symbol_and_errors_and_hints_keep_a_prefix() {
        assert_eq!(
            shown(Mark::Done, "created a fleet", false),
            "created a fleet"
        );
        assert_eq!(shown(Mark::Error, "no", false), "fleet: no");
        assert_eq!(
            shown(Mark::Warn, "could not tell", false),
            "fleet: could not tell"
        );
        assert_eq!(
            shown(Mark::Next, "run fleet start", false),
            "hint: run fleet start"
        );
    }

    #[test]
    fn a_terminal_row_is_a_bar_the_check_then_healthy_and_one_gray_detail() {
        console::set_colors_enabled(true);
        let report = [
            Found::StateOpen {
                dir: PathBuf::from("/s/fleet"),
                schema: 1,
            },
            Found::NotRunning { unit_serves: None },
        ];
        let raw = doctor_lines(&report, true);
        console::set_colors_enabled(false);
        let plain: Vec<String> = raw
            .iter()
            .map(|l| console::strip_ansi_codes(l).into_owned())
            .collect();
        assert_eq!(
            plain[0],
            format!("┃ {:<30}healthy /s/fleet: open at schema 1", "state")
        );
        assert_eq!(plain[1], format!("┃ {:<30}not running", "supervisor"));
        let detail = raw[0].split("healthy").nth(1).unwrap_or_default();
        let colours = detail.matches("\u{1b}[").count() - detail.matches("\u{1b}[0m").count();
        assert_eq!(colours, 1, "one colour for the whole detail: {detail:?}");
    }

    #[test]
    fn doctor_lists_every_result_before_any_hint() {
        let report = [
            Found::NoManager,
            Found::NotRunning { unit_serves: None },
            Found::StateUnknown,
        ];
        let lines = doctor_lines(&report, false);
        assert_eq!(lines.len(), 6, "{lines:?}");
        assert!(
            lines[..3].iter().all(|l| !l.starts_with("hint:")),
            "{lines:?}"
        );
        assert_eq!(lines[3], "");
        assert!(
            lines[4..].iter().all(|l| l.starts_with("hint:")),
            "{lines:?}"
        );
    }

    #[test]
    fn a_partial_start_is_a_done_line_then_an_error_line_per_fleet_left_off() {
        let off = |fleet: &str| protocol::response::StayedOff {
            fleet: PathBuf::from(fleet),
            why: protocol::Status::File(status::File::Missing {
                path: PathBuf::from(fleet).join("fleet.toml"),
            }),
        };
        let ending = Err(Status::PartlyOn {
            response: protocol::response::AllOn {
                on: vec![PathBuf::from("/code/app")],
                fleets: vec![PathBuf::from("/code/app")],
                picked_up: vec![],
                stayed_off: vec![off("/code/wing"), off("/code/blog")],
                forgotten: vec![PathBuf::from("/code/old")],
            },
            startup: crate::protocol::Startup::WasRunning,
        });
        let marks: Vec<Mark> = lines(&ending).into_iter().map(|(m, _)| m).collect();
        assert_eq!(
            marks,
            [Mark::Done, Mark::Warn, Mark::Error, Mark::Error, Mark::Next]
        );
    }

    #[test]
    fn terminal_lines_start_with_the_symbol_and_indent_the_rest() {
        let text = console::strip_ansi_codes(&shown(Mark::Done, "one\ntwo", true)).into_owned();
        assert_eq!(text, "◆ one\n  two");
        let text = console::strip_ansi_codes(&shown(Mark::Error, "no", true)).into_owned();
        assert_eq!(text, "■ no");
        let text = console::strip_ansi_codes(&shown(Mark::Next, "go", true)).into_owned();
        assert_eq!(text, "> go");
    }
}
