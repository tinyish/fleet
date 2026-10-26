//! `fleet specimen`: every line the CLI can print, through the same code a
//! command uses, so a person can look at all of them on a real terminal.
//! Compiled only with the `specimen` feature, so a release build has no such
//! command. Its labels are code identifiers, never sentences.

mod adapter;
mod doctor;
mod file;
mod fleet;
mod overview;
mod project;
mod refusals;
mod samples;
mod session;

use std::io::{self, Write};
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use cliclack::{Theme, ThemeState};

use crate::Args;
use crate::ui::theme::{FleetTheme, Tone, paint};
use crate::ui::{self, output, words};

use samples::{Sample, endings};

/// Prints every line: the help text, the start prompt in each state, the
/// spinners, then every way a command can end with its hint and exit code.
pub(crate) fn run() -> ExitCode {
    help();
    prompts();
    spinners();
    for (label, found) in doctor::doctor() {
        heading(&label);
        output::doctor(&[found]);
    }
    for Sample { label, ending } in endings() {
        let code = match &ending {
            Ok(_) => 0,
            Err(status) => status.exit_code(),
        };
        heading(&format!("{label} · exit {code}"));
        let _ = output::report(&ending);
        let _ = io::stdout().flush();
    }
    ExitCode::SUCCESS
}

/// A gray label on stderr, after flushing stdout so that a piped `2>&1`
/// keeps each label above its own lines.
fn heading(label: &str) {
    let _ = io::stdout().flush();
    let _ = writeln!(
        io::stderr(),
        "\n{}",
        paint(Tone::Muted).apply_to(format!("── {label}"))
    );
}

/// The help and version text exactly as clap prints it, by asking the real
/// parser for it.
fn help() {
    for argv in help_argvs() {
        heading(&argv.join(" "));
        if let Err(e) = Args::try_parse_from(&argv) {
            let _ = e.print();
        }
    }
}

/// The command lines whose help, version or usage text the specimen shows:
/// bare `fleet`, which prints help as an error, each `--help`, `--version`,
/// and one usage error.
fn help_argvs() -> Vec<Vec<String>> {
    let mut argvs = vec![
        vec!["fleet".to_owned()],
        vec!["fleet".to_owned(), "--help".to_owned()],
    ];
    argvs.push(vec!["fleet".to_owned(), "--version".to_owned()]);
    for sub in Args::command().get_subcommands() {
        argvs.push(vec![
            "fleet".to_owned(),
            sub.get_name().to_owned(),
            "--help".to_owned(),
        ]);
    }
    argvs.push(vec![
        "fleet".to_owned(),
        "create".to_owned(),
        "--bogus".to_owned(),
    ]);
    argvs
}

/// The start prompt in each state, drawn by the same theme calls cliclack
/// makes. `ThemeState::Error` is left out: a yes/no prompt never validates.
fn prompts() {
    let frames = [
        ("ThemeState::Active, Yes", ThemeState::Active, true),
        ("ThemeState::Active, No", ThemeState::Active, false),
        ("ThemeState::Submit, Yes", ThemeState::Submit, true),
        ("ThemeState::Submit, No", ThemeState::Submit, false),
        ("ThemeState::Cancel", ThemeState::Cancel, true),
    ];
    for (label, state, value) in frames {
        heading(&format!("ui::confirm · {label}"));
        let theme = FleetTheme;
        let _ = write!(
            io::stderr(),
            "{}{}{}",
            theme.format_header(&state, words::START_PROMPT),
            theme.format_confirm(&state, value),
            theme.format_footer(&state)
        );
    }
}

/// Each spinner, running for a moment. Piped, it draws nothing, as in a
/// real command.
fn spinners() {
    for message in [words::STARTING, words::STOPPING] {
        heading("ui::spinner");
        let spinner = ui::spinner(message);
        thread::sleep(Duration::from_millis(1500));
        spinner.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_shows_bare_fleet_and_a_usage_error() {
        let argvs = help_argvs();
        assert!(argvs.iter().any(|a| a == &["fleet"]));
        assert!(argvs.iter().any(|a| a.iter().any(|x| x == "--bogus")));
    }
}
