//! Creating a fleet.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::config::{self, Config};
use fleet_core::fleets::{CreateDecision, decide_create};
use fleet_core::protocol::{Reply, Response, Status, response};
use state::{State, Step};

use crate::{Error, lock, repo_root_of, unusable};

/// Creates a fleet, under the state lock so that two creates never race.
pub(crate) fn create(
    state: &Mutex<State>,
    directory: &Path,
    projects: &[PathBuf],
) -> Result<Reply, Error> {
    let directory = match fs::canonicalize(directory) {
        Ok(d) => d,
        Err(e) => return Ok(Reply::Status(unusable(directory, &e))),
    };
    let mut canonical = Vec::with_capacity(projects.len());
    for project in projects {
        match fs::canonicalize(project) {
            Ok(p) => canonical.push(p),
            Err(e) => return Ok(Reply::Status(unusable(project, &e))),
        }
    }
    let mut state = lock(state);
    let registered = state.fleets()?;
    let fleet = match decide_create(&registered, &directory, &canonical, repo_root_of) {
        CreateDecision::Create(fleet) => fleet,
        CreateDecision::AlreadyHere(fleet) => {
            return Ok(Reply::Response(Response::Create(response::Create {
                directory: fleet.directory,
                projects: fleet.projects,
                created: false,
            })));
        }
        CreateDecision::Refuse(status) => return Ok(Reply::Status(status)),
    };
    let path = directory.join("fleet.toml");
    if path.exists() {
        return Ok(Reply::Status(Status::StrayConfig { path }));
    }
    let text = config::render(&Config::for_fleet(&fleet));
    let created = Step::FleetCreated {
        directory: directory.display().to_string(),
        projects: fleet
            .projects
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    };
    let written = state.write(|w| {
        w.create_fleet(&fleet, || {
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .write_all(text.as_bytes())
        })?;
        w.record(&created)
    });
    match written {
        Ok(()) => {}
        Err(state::StateError::Config(e)) => {
            return Ok(Reply::Status(Status::CannotWrite {
                path,
                error: e.to_string(),
            }));
        }
        Err(e) => return Err(e.into()),
    }
    Ok(Reply::Response(Response::Create(response::Create {
        directory,
        projects: fleet.projects,
        created: true,
    })))
}
