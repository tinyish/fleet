//! `fleet start --project` and `fleet stop --project`: one project, off the
//! default path. The rest of its fleet keeps its own on or off.

use std::path::{Path, PathBuf};

use fleet_core::tools::ServiceManager;

use crate::cmd::start;
use crate::protocol::{Problem, Response, Status};
use crate::{Ask, client};

fn absolute(project: &Path) -> Result<PathBuf, Status> {
    std::path::absolute(project).map_err(|error| {
        Status::Cannot(Problem::BadPath {
            path: project.to_path_buf(),
            error,
        })
    })
}

/// Turns one project on, bringing the supervisor up first if it is down:
/// start is the verb that starts it, so it never asks.
pub(crate) fn start(
    state_dir: &Path,
    service: &dyn ServiceManager,
    project: &Path,
) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    let project = absolute(project)?;
    let startup = start::ensure_supervisor(state_dir, &socket, service, Ask::Yes)?;
    let response = client::start_project(&socket, project, startup)?;
    Ok(Response::ProjectOn { response, startup })
}

/// Turns one project off; the supervisor keeps running. With the supervisor
/// down nothing is on, and the project's stored on or off is left as it was.
pub(crate) fn stop(state_dir: &Path, project: &Path) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    let project = absolute(project)?;
    if !client::answers(&socket) {
        return Ok(Response::SupervisorWasNotRunning);
    }
    let response = client::stop_project(&socket, project)?;
    Ok(Response::ProjectOff(response))
}
