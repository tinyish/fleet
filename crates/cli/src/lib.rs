//! The command line: turns an invocation into what tinyfleet does and how it
//! ends.

mod client;
mod cmd;
#[cfg(feature = "debug")]
mod debug;
mod protocol;
#[cfg(feature = "specimen")]
mod specimen;
mod ui;

use std::ffi::OsString;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use fleet_core::paths::{Os, PathError, state_dir};
use fleet_core::tools::{Git, NoServiceManager, ServiceManager};

use crate::protocol::{Problem, Request, Status};
use cmd::{create, doctor, projects, remove, start, stop, uninstall};
use ui::{output, words};

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
    #[cfg(feature = "debug")]
    if args.get(1).is_some_and(|a| a == "debug") {
        return debug::run(&args[1..]);
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
        Request::Uninstall => {
            setup(service).and_then(|(service, dir)| uninstall::run(&dir, service, ask))
        }
        Request::Remove { projects } => {
            setup(service).and_then(|(service, dir)| remove::run(&dir, service, ask, projects))
        }
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

// Only without the feature: with it, `debug` is caught before clap.
#[cfg(all(test, not(feature = "debug")))]
mod release_tests {
    use super::*;

    #[test]
    fn debug_is_not_a_command_in_a_release_build() {
        let err = Args::try_parse_from(["fleet", "debug", "list"]).err();
        assert!(
            matches!(&err, Some(e) if e.kind() == clap::error::ErrorKind::InvalidSubcommand),
            "{err:?}"
        );
    }
}
