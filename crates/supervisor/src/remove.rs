//! Removing a fleet, or taking projects out of a wingman fleet.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use fleet_core::config;
use fleet_core::fleets::{Fleet, RemoveDecision, decide_remove};
use fleet_core::paths::CONFIG_FILE;
use fleet_core::protocol::request::{self, ConfigChoice};
use fleet_core::protocol::response::{self, ConfigFate};
use fleet_core::protocol::{Reply, Response, Status};
use fleet_core::status;
use state::{State, StateError, Step};

use crate::{Error, Supervisor, canonical};

/// Removes the fleet in `directory`, or takes `projects` out of it, under
/// the state lock so it never races a create or a start. A path that no
/// longer exists is taken as given, as `stop --project` does, so a fleet can
/// be removed after its repositories are gone.
pub(crate) fn remove(sup: &Supervisor, ask: request::Remove) -> Result<Reply, Error> {
    let request::Remove::Fleet {
        directory,
        projects,
        config,
    } = ask;
    let directory = canonical(&directory);
    let named: Vec<PathBuf> = projects.iter().map(|p| canonical(p)).collect();
    let mut state = sup.lock();
    let registered = state.fleets()?;
    match decide_remove(&registered, &directory, &named) {
        RemoveDecision::NothingHere => Ok(answer(response::Remove::NothingHere { directory })),
        RemoveDecision::Refuse(status) => Ok(Reply::Status(status)),
        RemoveDecision::Whole(fleet) => whole(&mut state, fleet, config),
        RemoveDecision::Take { fleet, taken } => take(&mut state, fleet, taken),
    }
}

fn answer(remove: response::Remove) -> Reply {
    Reply::Response(Response::Remove(remove))
}

fn shown(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

/// The whole fleet goes; its config file too when `choice` says to delete
/// it. Asked to ask first while the file is there, it changes nothing and
/// answers `Ask`. The file is deleted inside the write, so a delete that
/// fails changes nothing.
fn whole(state: &mut State, fleet: Fleet, choice: ConfigChoice) -> Result<Reply, Error> {
    let path = fleet.directory.join(CONFIG_FILE);
    let found = if path.exists() {
        ConfigFate::Kept
    } else {
        ConfigFate::Missing
    };
    if found == ConfigFate::Kept && choice == ConfigChoice::Ask {
        return Ok(answer(response::Remove::Ask {
            directory: fleet.directory,
        }));
    }
    let delete_config = choice == ConfigChoice::Delete;
    let written = state.write(|w| {
        let config = match found {
            ConfigFate::Kept if delete_config => match fs::remove_file(&path) {
                Ok(()) => ConfigFate::Deleted,
                Err(e) if e.kind() == io::ErrorKind::NotFound => ConfigFate::Missing,
                Err(e) => return Err(StateError::Config(e)),
            },
            other => other,
        };
        let removed = Step::FleetRemoved {
            directory: fleet.directory.display().to_string(),
            projects: shown(&fleet.projects),
            config,
        };
        w.remove_fleet(&fleet.directory, &removed)?;
        Ok(config)
    });
    let config = match written {
        Ok(config) => config,
        Err(StateError::Config(e)) => return Ok(cannot_write(path, &e)),
        Err(e) => return Err(e.into()),
    };
    Ok(answer(response::Remove::Removed {
        directory: fleet.directory,
        projects: fleet.projects,
        config,
    }))
}

/// Takes `taken` out of the wingman fleet: its config file loses their
/// entries, kept as the person wrote the rest, and the registry loses them.
fn take(state: &mut State, fleet: Fleet, taken: Vec<PathBuf>) -> Result<Reply, Error> {
    let path = fleet.directory.join(CONFIG_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return Ok(Reply::Status(Status::unusable(&path, &e))),
    };
    let directory = &fleet.directory;
    let edited = config::without_projects(&text, |entry| {
        taken.contains(&canonical(&directory.join(entry)))
    });
    let edited = match edited {
        Ok(edited) => edited,
        Err(e) => {
            return Ok(Reply::Status(Status::File(status::File::BadConfig {
                path,
                line: e.line,
                column: e.column,
                error: e.message,
            })));
        }
    };
    let written = state.write(|w| {
        w.remove_projects(&taken)?;
        replace(&path, &edited).map_err(StateError::Config)?;
        w.record(&Step::ProjectsTaken {
            fleet: directory.display().to_string(),
            projects: shown(&taken),
        })
    });
    match written {
        Ok(()) => {}
        Err(StateError::Config(e)) => return Ok(cannot_write(path, &e)),
        Err(e) => return Err(e.into()),
    }
    let remaining = fleet
        .projects
        .iter()
        .filter(|p| !taken.contains(p))
        .cloned()
        .collect();
    Ok(answer(response::Remove::ProjectsTaken {
        directory: fleet.directory,
        taken,
        remaining,
    }))
}

/// Writes `text` over `path` through a temporary file and a rename, so the
/// file is never half written.
fn replace(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

fn cannot_write(path: PathBuf, error: &io::Error) -> Reply {
    Reply::Status(Status::File(status::File::CannotWrite {
        path,
        error: error.to_string(),
    }))
}
