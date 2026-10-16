//! `fleet stop`: every fleet off, the supervisor down and disabled.

use std::path::Path;

use fleet_core::tools::ServiceManager;
use protocol::request;

use crate::ending::{Doing, Doubt, Problem, Response, Status};
use crate::{client, ui, words};

pub(crate) fn run(state_dir: &Path, service: &dyn ServiceManager) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    let was_running = client::answers(&socket);
    if was_running {
        client::stop(&socket, request::Stop::All)?;
    }
    service.stop_and_disable(state_dir).map_err(|error| {
        Status::Cannot(Problem::ServiceManager {
            doing: Doing::Stop,
            error,
        })
    })?;
    if !was_running {
        return Ok(Response::SupervisorWasNotRunning);
    }
    let spinner = ui::spinner(words::STOPPING);
    let gone = client::wait_until(&socket, false);
    spinner.clear();
    if gone {
        Ok(Response::SupervisorStopped)
    } else {
        Err(Status::CouldNotTell(Doubt::StillAnswers {
            waited: client::WAIT,
        }))
    }
}
