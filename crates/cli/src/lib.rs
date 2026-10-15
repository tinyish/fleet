//! The command line: turns an invocation into what tinyfleet does and how it
//! ends.

use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser;

/// tinyfleet runs AI coding agents on your projects.
#[derive(Debug, Parser)]
#[command(name = "fleet", version, arg_required_else_help = true)]
struct Args {}

/// Runs one invocation of `fleet` and returns how it ended.
pub fn run<I, T>(args: I) -> ExitCode
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match Args::try_parse_from(args) {
        Ok(Args {}) => ExitCode::SUCCESS,
        Err(err) => {
            let code = err.exit_code();
            // A reader that closed the pipe early is not an error worth a panic.
            let _ = err.print();
            ExitCode::from(u8::try_from(code).unwrap_or(2))
        }
    }
}
