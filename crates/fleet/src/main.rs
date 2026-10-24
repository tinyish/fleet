//! The `fleet` binary: the supervisor when run as `fleet supervise`, the
//! CLI otherwise.

use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args_os().nth(1).is_some_and(|a| a == "supervise") {
        return supervisor::main(std::env::args_os().skip(2));
    }
    let service = service::for_this_os();
    cli::run(
        std::env::args_os(),
        service.as_deref().map_err(|e| *e),
        &git::Git,
    )
}
