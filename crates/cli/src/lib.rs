//! The command line: turns an invocation into what tinyfleet does and how it
//! ends.

mod client;
mod create;
mod doctor;
mod ending;
mod output;
mod projects;
#[cfg(feature = "specimen")]
mod specimen;
mod start;
mod stop;
mod theme;
mod ui;
mod words;

use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fleet_core::paths::{Os, PathError, state_dir};
use fleet_core::tools::{Git, NoServiceManager, ServiceManager};

use crate::ending::{Problem, Status};

// The invocation as clap parses it. Its help text is in words.rs.
#[derive(Debug, Parser)]
#[command(
    name = "fleet",
    version,
    about = words::ABOUT,
    arg_required_else_help = true
)]
struct Args {
    #[arg(long, global = true, help = words::NON_INTERACTIVE_HELP)]
    non_interactive: bool,
    #[command(subcommand)]
    request: Request,
}

// What the person asked the CLI to do. The CLI's side of the vocabulary:
// a `Request` in, `Result<Response, Status>` out (see `ending`). Doc
// comments here would become help text, so these are plain comments.
#[derive(Debug, Subcommand)]
enum Request {
    // `fleet create`, with any `--project` paths.
    #[command(about = words::CREATE_ABOUT)]
    Create {
        #[arg(long = "project", value_name = "PATH", help = words::PROJECT_HELP)]
        projects: Vec<PathBuf>,
    },
    // `fleet doctor`.
    #[command(about = words::DOCTOR_ABOUT)]
    Doctor,
    // `fleet start`, or `fleet start --project` for one project.
    #[command(about = words::START_ABOUT)]
    Start {
        #[arg(long, value_name = "PATH", help = words::START_PROJECT_HELP)]
        project: Option<PathBuf>,
    },
    // `fleet stop`, or `fleet stop --project` for one project.
    #[command(about = words::STOP_ABOUT)]
    Stop {
        #[arg(long, value_name = "PATH", help = words::STOP_PROJECT_HELP)]
        project: Option<PathBuf>,
    },
}

/// Runs one invocation of `fleet` and returns how it ended. `service` is
/// this machine's service manager, or why there is none; `git` is the
/// person's git.
pub fn run<I, T>(
    args: I,
    service: Result<&dyn ServiceManager, NoServiceManager>,
    git: &dyn Git,
) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    #[cfg(feature = "specimen")]
    if args.get(1).is_some_and(|a| a == "specimen") {
        return specimen::run();
    }
    let args = match Args::try_parse_from(args) {
        Ok(args) => args,
        Err(err) => {
            let code = err.exit_code();
            // A reader that closed the pipe early is not an error worth a panic.
            let _ = err.print();
            return ExitCode::from(u8::try_from(code).unwrap_or(2));
        }
    };
    let ask = if args.non_interactive {
        Ask::Yes
    } else if io::stdin().is_terminal() && io::stderr().is_terminal() {
        Ask::Terminal
    } else {
        Ask::Never
    };
    let ending = match &args.request {
        Request::Doctor => return doctor::run(service, git),
        Request::Create { projects } => {
            setup(service).and_then(|(service, dir)| create::run(&dir, service, ask, projects))
        }
        Request::Start { project: None } => {
            setup(service).and_then(|(service, dir)| start::run(&dir, service))
        }
        Request::Start {
            project: Some(project),
        } => setup(service).and_then(|(service, dir)| projects::start(&dir, service, project)),
        Request::Stop { project: None } => {
            setup(service).and_then(|(service, dir)| stop::run(&dir, service))
        }
        Request::Stop {
            project: Some(project),
        } => setup(service).and_then(|(_, dir)| projects::stop(&dir, project)),
    };
    output::report(&ending)
}

/// What every command but doctor needs first: a system tinyfleet runs on,
/// its service manager, and the state directory.
fn setup(
    service: Result<&dyn ServiceManager, NoServiceManager>,
) -> Result<(&dyn ServiceManager, PathBuf), Status> {
    let service = service.map_err(|e| {
        Status::Cannot(match e {
            NoServiceManager::UnsupportedOs => Problem::UnsupportedOs,
            NoServiceManager::NoHome => Problem::StateDir(PathError::NoHome),
        })
    })?;
    let os = Os::this().ok_or(Status::Cannot(Problem::UnsupportedOs))?;
    let dir = state_dir(os, |name| std::env::var_os(name))
        .map_err(|e| Status::Cannot(Problem::StateDir(e)))?;
    Ok((service, dir))
}

/// Whether a command may ask before doing something.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ask {
    /// `--non-interactive`: never ask, take the answer that keeps going.
    Yes,
    /// A person at a terminal: ask.
    Terminal,
    /// No terminal and no `--non-interactive`: never ask, and do not go on.
    Never,
}

// Only without the feature: with it, `specimen` is caught before clap.
#[cfg(all(test, not(feature = "specimen")))]
mod tests {
    use super::*;

    #[test]
    fn specimen_is_not_a_command_in_a_release_build() {
        let err = Args::try_parse_from(["fleet", "specimen"]).err();
        assert!(
            matches!(&err, Some(e) if e.kind() == clap::error::ErrorKind::InvalidSubcommand),
            "{err:?}"
        );
    }
}
