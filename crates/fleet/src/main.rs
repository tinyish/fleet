//! The `fleet` binary: the supervisor when run as `fleet supervise`, the
//! CLI otherwise.

use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args_os().nth(1).is_some_and(|a| a == "supervise") {
        return supervisor::main(std::env::args_os().skip(2), |state_dir| supervisor::Tools {
            mux: Box::new(tmux::Tmux::new(state_dir.join(tmux::SOCKET))),
            agents: Box::new(adapter_host::Host::default()),
            // file:// releases only when a base of its own is given.
            net: Box::new(curl::Curl::new(
                std::env::var_os(fleet_core::adapters::BASE_VAR).is_some(),
            )),
        });
    }
    let service = service::for_this_os();
    cli::run(
        std::env::args_os(),
        service.as_deref().map_err(|e| *e),
        &git::Git,
        &curl::Curl::new(false),
    )
}
