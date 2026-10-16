//! `fleet create`: a fleet in the current directory, a default fleet or,
//! with `--project`, a wingman fleet.

use std::path::{Path, PathBuf};

use fleet_core::tools::ServiceManager;
use protocol::request;

use crate::ending::{Problem, Response, Status};
use crate::{Ask, client, start};

pub(crate) fn run(
    state_dir: &Path,
    service: &dyn ServiceManager,
    ask: Ask,
    projects: &[PathBuf],
) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
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
    let startup = start::ensure_supervisor(state_dir, &socket, service, ask)?;
    let request = request::Create::Fleet {
        directory,
        projects: absolute,
    };
    let response = client::create(&socket, request)?;
    Ok(Response::Created { response, startup })
}
