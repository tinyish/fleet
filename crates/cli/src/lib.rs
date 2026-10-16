//! The command line: turns an invocation into what tinyfleet does and how it
//! ends.

mod client;
mod start;
mod stop;

use std::ffi::OsString;
use std::io::{self, Write};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use fleet_core::outcome::Outcome;
use fleet_core::paths::{Os, state_dir};
use fleet_core::tools::ServiceManager;

/// tinyfleet runs AI coding agents on your projects.
#[derive(Debug, Parser)]
#[command(name = "fleet", version, arg_required_else_help = true)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Start the supervisor and turn every fleet on; it comes back after a reboot until stop
    Start,
    /// Turn every fleet off and stop the supervisor, so it does not come back
    Stop,
}

/// Runs one invocation of `fleet` and returns how it ended. `service` is
/// this machine's service manager, or `None` where tinyfleet does not run.
pub fn run<I, T>(args: I, service: Option<&dyn ServiceManager>) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args = match Args::try_parse_from(args) {
        Ok(args) => args,
        Err(err) => {
            let code = err.exit_code();
            // A reader that closed the pipe early is not an error worth a panic.
            let _ = err.print();
            return ExitCode::from(u8::try_from(code).unwrap_or(2));
        }
    };
    report(&decide(&args, service))
}

fn decide(args: &Args, service: Option<&dyn ServiceManager>) -> Outcome {
    let (Some(os), Some(service)) = (Os::this(), service) else {
        return Outcome::Refused("tinyfleet runs on macOS and Linux only".into());
    };
    let dir = match state_dir(os, |name| std::env::var_os(name)) {
        Ok(dir) => dir,
        Err(e) => return Outcome::Refused(e.to_string()),
    };
    match args.command {
        Command::Start => start::run(&dir, service),
        Command::Stop => stop::run(&dir, service),
    }
}

fn report(outcome: &Outcome) -> ExitCode {
    let _ = match outcome {
        Outcome::Done(said) => writeln!(io::stdout(), "{said}"),
        Outcome::Refused(why) | Outcome::CouldNotTell(why) => {
            writeln!(io::stderr(), "fleet: {why}")
        }
        Outcome::SupervisorNotRunning => {
            writeln!(io::stderr(), "fleet: the supervisor is not running")
        }
    };
    ExitCode::from(outcome.exit_code())
}
