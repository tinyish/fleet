//! The `fleet` binary: the supervisor when run as `fleet supervise`, the
//! CLI otherwise.

use std::io::{self, Write};
use std::process::ExitCode;

use fleet_core::paths::{Os, state_dir};

fn main() -> ExitCode {
    if std::env::args_os().nth(1).is_some_and(|a| a == "supervise") {
        return supervise();
    }
    let service = service::for_this_os();
    cli::run(std::env::args_os(), service.as_deref())
}

fn supervise() -> ExitCode {
    let Some(os) = Os::this() else {
        let _ = writeln!(
            io::stderr(),
            "fleet: tinyfleet runs on macOS and Linux only"
        );
        return ExitCode::FAILURE;
    };
    match state_dir(os, |name| std::env::var_os(name)) {
        Ok(dir) => supervisor::run(&dir),
        Err(e) => {
            let _ = writeln!(io::stderr(), "fleet supervise: {e}");
            ExitCode::FAILURE
        }
    }
}
