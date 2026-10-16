//! Where a command's words go, and the ink they are written in. Each line
//! carries a mark: on a terminal the mark is a coloured symbol at the start
//! of the line; piped, there is no symbol: warnings and errors start with
//! `fleet:` instead, and hints with `hint:`. Colour, like the symbol, shows only on a terminal, so a
//! script reads plain text.

use std::fmt::Display;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::ending::{Response, Status};

use crate::theme::{ERROR, NEXT, SUCCESS, Tone, WARN, paint_on};
use crate::words;

/// The ink for one stream: colour is decided by whether that stream is a
/// terminal.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ink {
    stderr: bool,
}

/// Ink for done lines, on stdout.
pub(crate) const OUT: Ink = Ink { stderr: false };

/// Ink for refusals and what could not be told, on stderr.
pub(crate) const ERR: Ink = Ink { stderr: true };

impl Ink {
    fn tone(self, tone: Tone, text: impl Display) -> String {
        paint_on(tone, self.stderr).apply_to(text).to_string()
    }

    /// What to notice: a path, a name, a command.
    pub(crate) fn accent(self, text: impl Display) -> String {
        self.tone(Tone::Accent, text)
    }

    /// One path, as something to notice.
    pub(crate) fn path(self, path: &Path) -> String {
        self.accent(path.display())
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

/// What a line is, shown by the symbol it starts with on a terminal. More
/// marks (info `●`) come with the first line that needs one.
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
}

impl Mark {
    fn on_stderr(self) -> bool {
        !matches!(self, Mark::Done)
    }

    /// What starts the line in place of the symbol when it is piped.
    fn piped_prefix(self) -> &'static str {
        match self {
            Mark::Done => "",
            Mark::Warn | Mark::Error => "fleet: ",
            Mark::Next => "hint: ",
        }
    }

    fn symbol(self) -> (&'static str, Tone) {
        match self {
            Mark::Done => (SUCCESS, Tone::Good),
            Mark::Warn => (WARN, Tone::Warn),
            Mark::Error => (ERROR, Tone::Bad),
            Mark::Next => (NEXT, Tone::Accent),
        }
    }
}

/// The line `text` as `mark` shows it on a stream that `terminal` or not:
/// the coloured symbol on a terminal; piped, `fleet:` before a warning or
/// error, `hint:` before a hint, nothing before a done line. Lines after the first are
/// indented under the text on a terminal.
fn shown(mark: Mark, text: &str, terminal: bool) -> String {
    if terminal {
        let (symbol, tone) = mark.symbol();
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

/// Prints how a command ended, in words, then what to do next when there is
/// something to do, and returns its exit code.
pub(crate) fn report(ending: &Result<Response, Status>) -> ExitCode {
    match ending {
        Ok(response) => {
            line(Mark::Done, &words::response(response, OUT));
            ExitCode::SUCCESS
        }
        Err(status) => {
            let mark = match status {
                Status::CouldNotTell(_) => Mark::Warn,
                _ => Mark::Error,
            };
            line(mark, &words::status(status, ERR));
            if let Some(next) = words::next(status, ERR) {
                line(Mark::Next, &next);
            }
            ExitCode::from(status.exit_code())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn terminal_lines_start_with_the_symbol_and_indent_the_rest() {
        let text = console::strip_ansi_codes(&shown(Mark::Done, "one\ntwo", true)).into_owned();
        assert_eq!(text, "◆ one\n  two");
        let text = console::strip_ansi_codes(&shown(Mark::Error, "no", true)).into_owned();
        assert_eq!(text, "■ no");
        let text = console::strip_ansi_codes(&shown(Mark::Next, "go", true)).into_owned();
        assert_eq!(text, "> go");
    }
}
