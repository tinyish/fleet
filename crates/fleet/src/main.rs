//! The `fleet` binary. It plays the CLI role; the supervisor role arrives
//! with fleet-lc94.2.

use std::process::ExitCode;

fn main() -> ExitCode {
    cli::run(std::env::args_os())
}
