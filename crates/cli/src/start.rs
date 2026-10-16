//! `fleet start`: the supervisor up, every fleet on.

use std::path::Path;

use fleet_core::outcome::Outcome;
use fleet_core::tools::{ServiceManager, SupervisorUnit};
use protocol::{Answer, Request};

use crate::client;

pub(crate) fn run(state_dir: &Path, service: &dyn ServiceManager) -> Outcome {
    let socket = match protocol::socket_path(state_dir) {
        Ok(s) => s,
        Err(e) => return Outcome::Refused(e.to_string()),
    };
    let was_running = client::answers(&socket);
    if !was_running {
        let program = match std::env::current_exe() {
            Ok(p) => p,
            Err(e) => return Outcome::Refused(format!("cannot find this fleet binary: {e}")),
        };
        let unit = SupervisorUnit {
            program,
            state_dir: state_dir.to_path_buf(),
        };
        if let Err(e) = service.enable_and_start(&unit) {
            return Outcome::Refused(format!(
                "the service manager did not start the supervisor: {e}"
            ));
        }
        if !client::wait_until(&socket, true) {
            return Outcome::CouldNotTell(format!(
                "the supervisor did not answer within {}s; its log: {}",
                client::WAIT.as_secs(),
                service.where_logs_go(state_dir)
            ));
        }
    }
    let first = if was_running {
        "the supervisor was already running"
    } else {
        "the supervisor is running"
    };
    match client::ask(&socket, &Request::AllOn) {
        Ok(Answer::Done { said }) => Outcome::Done(format!("{first}; {said}")),
        Ok(Answer::Refused { reason }) => Outcome::Refused(reason),
        Ok(Answer::Hello { .. }) => {
            Outcome::CouldNotTell("the supervisor answered hello to all_on".into())
        }
        Err(e) => Outcome::CouldNotTell(format!("{first}, but turning every fleet on: {e}")),
    }
}
