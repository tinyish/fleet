//! `fleet stop`: every fleet off, the supervisor down and disabled.

use std::path::Path;

use fleet_core::outcome::Outcome;
use fleet_core::tools::ServiceManager;
use protocol::{Answer, Request};

use crate::client;

pub(crate) fn run(state_dir: &Path, service: &dyn ServiceManager) -> Outcome {
    let socket = match protocol::socket_path(state_dir) {
        Ok(s) => s,
        Err(e) => return Outcome::Refused(e.to_string()),
    };
    let was_running = client::answers(&socket);
    if was_running {
        match client::ask(&socket, &Request::AllOff) {
            Ok(Answer::Done { .. }) => {}
            Ok(Answer::Refused { reason }) => return Outcome::Refused(reason),
            Ok(Answer::Hello { .. }) => {
                return Outcome::CouldNotTell("the supervisor answered hello to all_off".into());
            }
            Err(e) => return Outcome::CouldNotTell(format!("turning every fleet off: {e}")),
        }
    }
    if let Err(e) = service.stop_and_disable(state_dir) {
        return Outcome::Refused(format!(
            "the service manager did not stop the supervisor: {e}"
        ));
    }
    if !was_running {
        return Outcome::Done("the supervisor was not running".into());
    }
    if client::wait_until(&socket, false) {
        Outcome::Done("every fleet is off; the supervisor is stopped".into())
    } else {
        Outcome::CouldNotTell(format!(
            "the supervisor still answers after {}s",
            client::WAIT.as_secs()
        ))
    }
}
