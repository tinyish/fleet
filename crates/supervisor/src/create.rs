//! Creating a fleet.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use fleet_core::config::{self, Config};
use fleet_core::fleets::{self, CreateDecision, decide_create};
use fleet_core::protocol::{Reply, Response, Status, request, response};
use fleet_core::seats::Roster;
use fleet_core::status;

use crate::{Error, Supervisor, repo_root_of};

/// Creates a fleet, under the state lock so that two creates never race.
pub(crate) fn create(sup: &Supervisor, ask: request::Create) -> Result<Reply, Error> {
    let request::Create::Fleet {
        directory,
        projects,
    } = ask;
    let directory = match fs::canonicalize(&directory) {
        Ok(d) => d,
        Err(e) => return Ok(Reply::Status(Status::unusable(&directory, &e))),
    };
    let mut canonical = Vec::with_capacity(projects.len());
    for project in &projects {
        match fs::canonicalize(project) {
            Ok(p) => canonical.push(p),
            Err(e) => return Ok(Reply::Status(Status::unusable(project, &e))),
        }
    }
    let mut state = sup.lock();
    let registered = state.fleets()?;
    let path = directory.join(fleet_core::paths::CONFIG_FILE);
    // A file no fleet owns is adopted: the fleet is the one it describes.
    let adopting = path.exists() && !registered.iter().any(|f| f.directory == directory);
    let (asked, roster) = if adopting {
        match adopted(&directory, &canonical) {
            Ok(adopted) => adopted,
            Err(status) => return Ok(Reply::Status(status)),
        }
    } else {
        (canonical, Roster::default())
    };
    let fleet = match decide_create(&registered, &directory, &asked, repo_root_of) {
        CreateDecision::Create(fleet) => fleet,
        CreateDecision::AlreadyHere(fleet) => {
            return Ok(Reply::Response(Response::Create(response::Create {
                directory: fleet.directory,
                projects: fleet.projects,
                created: false,
                adopted: false,
            })));
        }
        CreateDecision::Refuse(status) => return Ok(Reply::Status(status)),
    };
    let text = config::render(&Config::for_fleet(&fleet));
    let written = state.write(|w| {
        w.create_fleet(&fleet, &roster, adopting, || {
            if adopting {
                return Ok(());
            }
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .write_all(text.as_bytes())
        })
    });
    match written {
        Ok(()) => {}
        Err(state::StateError::Config(e)) => {
            return Ok(Reply::Status(Status::File(status::File::CannotWrite {
                path,
                error: e.to_string(),
            })));
        }
        Err(e) => return Err(e.into()),
    }
    Ok(Reply::Response(Response::Create(response::Create {
        directory,
        projects: fleet.projects,
        created: true,
        adopted: adopting,
    })))
}

/// The projects to create from the `fleet.toml` already in `directory`,
/// read as start reads it, and its roles and seats. Projects `asked` for
/// must be the ones it lists. A default fleet's file lists its own
/// directory, which create takes as no projects.
fn adopted(directory: &Path, asked: &[PathBuf]) -> Result<(Vec<PathBuf>, Roster), Status> {
    let (listed, roster) = fleets::read_fleet(
        directory,
        |p| fs::read_to_string(p),
        |p| fs::canonicalize(p),
        repo_root_of,
    )?;
    if !asked.is_empty() {
        let mut asked = asked.to_vec();
        asked.sort();
        asked.dedup();
        if asked != listed {
            return Err(Status::File(status::File::ConfigDiffers {
                path: directory.join(fleet_core::paths::CONFIG_FILE),
                listed,
            }));
        }
    }
    if listed == [directory] {
        return Ok((Vec::new(), roster));
    }
    Ok((listed, roster))
}
