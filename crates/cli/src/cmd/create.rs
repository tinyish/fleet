//! `fleet create`: a fleet in the current directory, a default fleet or,
//! with `--project`, a wingman fleet.

use std::path::{Path, PathBuf};

use fleet_core::protocol::{self, request};
use fleet_core::tools::ServiceManager;

use crate::Ask;
use crate::cmd::start;
use crate::protocol::{Problem, Response, Status};

pub(crate) fn run(
    state_dir: &Path,
    service: &dyn ServiceManager,
    ask: Ask,
    projects: &[PathBuf],
) -> Result<Response, Status> {
    let directory =
        std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    let mut absolute = Vec::with_capacity(projects.len());
    for project in projects {
        let path = std::path::absolute(project).map_err(|error| {
            Status::Cannot(Problem::BadPath {
                path: project.clone(),
                error,
            })
        })?;
        absolute.push(path);
    }
    let supervisor = start::supervisor(state_dir, service, ask)?;
    let request = request::Create::Fleet {
        directory,
        projects: absolute,
    };
    let response = supervisor.ask(&protocol::Request::Create(request), |answer| match answer {
        protocol::Response::Create(created) => Some(created),
        _ => None,
    })?;
    let startup = supervisor.startup;
    Ok(Response::Created { response, startup })
}
