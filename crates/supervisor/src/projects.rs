//! Turning projects on and off, re-reading each fleet's config on the way.
//! On and off belong to each project, not to its fleet.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::fleets::{self, Fleet, conflicts};
use fleet_core::protocol::{Reply, Response, Status, response};
use state::{State, StateError, Step, Write};

use crate::{Error, lock, repo_root_of};

fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// The fleet as its config now describes it: read, parsed and resolved, but
/// not yet checked against other fleets.
fn read_resolved(fleet: &Fleet) -> Result<Fleet, Status> {
    let projects = fleets::read_projects(
        &fleet.directory,
        |p| fs::read_to_string(p),
        |p| fs::canonicalize(p),
        repo_root_of,
    )?;
    Ok(Fleet {
        directory: fleet.directory.clone(),
        projects,
    })
}

/// What re-reading every fleet's config would change, before any of it is
/// applied.
struct Reread {
    /// Fleets whose config read, as it now describes them.
    ok: Vec<Fleet>,
    /// Fleets whose config did not read or clashed, as stored, with why.
    broken: Vec<(Fleet, Status)>,
    /// What each fleet that read gains and loses.
    changes: Vec<(PathBuf, fleets::Change)>,
}

/// Re-reads every fleet's config, in two phases: first every config is
/// read and resolved, then claims are checked against what every fleet now
/// wants, so a project moved between fleets in one edit is no conflict.
/// Nothing is written: `apply` writes it.
fn reread_all(state: &State) -> Result<Reread, Error> {
    let registered = state.fleets()?;
    let mut read = Vec::new();
    let mut broken = Vec::new();
    for fleet in registered {
        match read_resolved(&fleet) {
            Ok(wanted) => read.push((fleet, wanted)),
            Err(why) => broken.push((fleet, why)),
        }
    }
    let claims: Vec<Fleet> = read
        .iter()
        .map(|(_, wanted)| wanted.clone())
        .chain(broken.iter().map(|(stored, _)| stored.clone()))
        .collect();
    let clashes = conflicts(&claims);
    let mut ok = Vec::new();
    let mut changes = Vec::new();
    for (stored, wanted) in read {
        if let Some((_, why)) = clashes.iter().find(|(dir, _)| *dir == stored.directory) {
            broken.push((stored, why.clone()));
        } else {
            let change = fleets::sync(&stored.projects, &wanted.projects);
            changes.push((stored.directory.clone(), change));
            ok.push(wanted);
        }
    }
    Ok(Reread {
        ok,
        broken,
        changes,
    })
}

impl Reread {
    /// What the re-read changed, for the answer.
    fn picked_up(&self) -> Vec<response::PickedUp> {
        self.changes
            .iter()
            .filter(|(_, c)| !c.added.is_empty() || !c.removed.is_empty())
            .map(|(fleet, c)| response::PickedUp {
                fleet: fleet.clone(),
                added: c.added.clone(),
                removed: c.removed.clone(),
            })
            .collect()
    }

    /// Writes the re-read and records its steps, inside the caller's
    /// transaction. A project only added comes in off; a moved one keeps its
    /// on or off.
    fn apply(&self, w: &mut Write<'_>) -> Result<(), StateError> {
        w.apply_reread(&self.changes)?;
        for (fleet, change) in &self.changes {
            for project in &change.removed {
                w.record(&Step::ProjectRemoved {
                    fleet: shown(fleet),
                    project: shown(project),
                })?;
            }
            for project in &change.added {
                w.record(&Step::ProjectAdded {
                    fleet: shown(fleet),
                    project: shown(project),
                })?;
            }
        }
        for (fleet, why) in &self.broken {
            w.record(&Step::FleetConfigBroken {
                fleet: shown(&fleet.directory),
                why: why.clone(),
            })?;
        }
        Ok(())
    }
}

/// Re-reads every fleet's config and turns every project on. A fleet whose
/// config does not read, or claims another fleet's project, stays off and is
/// named in the answer.
pub(crate) fn all_on(state: &Mutex<State>) -> Result<Reply, Error> {
    let mut state = lock(state);
    let reread = reread_all(&state)?;
    state.write(|w| {
        reread.apply(w)?;
        for fleet in &reread.ok {
            w.set_fleet_active(&fleet.directory, true)?;
        }
        for (fleet, _) in &reread.broken {
            w.set_fleet_active(&fleet.directory, false)?;
        }
        w.record(&Step::EveryFleetOn)
    })?;
    let all_on = response::AllOn {
        on: reread
            .ok
            .iter()
            .flat_map(|f| f.projects.iter().cloned())
            .collect(),
        fleets: reread.ok.iter().map(|f| f.directory.clone()).collect(),
        picked_up: reread.picked_up(),
        stayed_off: reread
            .broken
            .into_iter()
            .map(|(fleet, why)| response::StayedOff {
                fleet: fleet.directory,
                why,
            })
            .collect(),
    };
    Ok(Reply::Response(Response::Start(response::Start::AllOn(
        all_on,
    ))))
}

/// Turns every project off.
pub(crate) fn all_off(state: &Mutex<State>) -> Result<Reply, Error> {
    let mut state = lock(state);
    state.write(|w| {
        w.set_all_active(false)?;
        w.record(&Step::EveryFleetOff)
    })?;
    Ok(Reply::Response(Response::Stop(response::Stop::AllOff)))
}

/// Why no fleet's project is `path`: it is inside one, or nobody serves it.
fn not_served(fleets: &[Fleet], path: &Path) -> Status {
    let inside = fleets
        .iter()
        .flat_map(|f| f.projects.iter())
        .find(|p| path.starts_with(p));
    match inside {
        Some(project) => Status::InsideProject {
            path: path.to_path_buf(),
            project: project.clone(),
        },
        None => Status::NotServed {
            path: path.to_path_buf(),
        },
    }
}

/// Turns one project on, after re-reading every fleet's config so a project
/// just added to one is found. No other project's on or off changes, even
/// when its fleet's config does not read. A refusal changes nothing: the
/// re-read is applied only when the project comes on.
pub(crate) fn project_on(state: &Mutex<State>, project: &Path) -> Result<Reply, Error> {
    let project = match fs::canonicalize(project) {
        Ok(p) => p,
        Err(e) => return Ok(Reply::Status(Status::unusable(project, &e))),
    };
    let mut state = lock(state);
    let reread = reread_all(&state)?;
    if let Some((fleet, why)) = reread
        .broken
        .iter()
        .find(|(f, _)| f.projects.contains(&project))
    {
        return Ok(Reply::Status(Status::CannotTurnOn {
            project,
            fleet: fleet.directory.clone(),
            why: Box::new(why.clone()),
        }));
    }
    if !reread.ok.iter().any(|f| f.projects.contains(&project)) {
        let dropped = reread
            .changes
            .iter()
            .find(|(_, c)| c.removed.contains(&project));
        if let Some((fleet, _)) = dropped {
            return Ok(Reply::Status(Status::NoLongerServed {
                project,
                fleet: fleet.clone(),
            }));
        }
        let all: Vec<Fleet> = reread
            .ok
            .iter()
            .cloned()
            .chain(reread.broken.iter().map(|(f, _)| f.clone()))
            .collect();
        return Ok(Reply::Status(not_served(&all, &project)));
    }
    let on = Step::ProjectOn {
        project: shown(&project),
    };
    state.write(|w| {
        reread.apply(w)?;
        w.set_active(&project, true)?;
        w.record(&on)
    })?;
    Ok(Reply::Response(Response::Start(
        response::Start::ProjectOn(response::ProjectOn {
            project,
            picked_up: reread.picked_up(),
        }),
    )))
}

/// Turns one project off; the rest of its fleet keeps its own on or off. A
/// project whose repository is gone is still found by the path given.
pub(crate) fn project_off(state: &Mutex<State>, project: &Path) -> Result<Reply, Error> {
    let project = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
    let mut state = lock(state);
    let registered = state.fleets()?;
    let Some(fleet) = registered
        .iter()
        .find(|f| f.projects.contains(&project))
        .cloned()
    else {
        return Ok(Reply::Status(not_served(&registered, &project)));
    };
    let off = Step::ProjectOff {
        project: shown(&project),
    };
    state.write(|w| {
        w.set_active(&project, false)?;
        w.record(&off)
    })?;
    let others = fleet
        .projects
        .iter()
        .filter(|p| **p != project)
        .cloned()
        .collect();
    Ok(Reply::Response(Response::Stop(response::Stop::ProjectOff(
        response::ProjectOff { project, others },
    ))))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp tree with repositories `names` under `code/`, and a state
    /// file under `st/`.
    fn tree(name: &str, names: &[&str]) -> (PathBuf, Mutex<State>) {
        let root = std::env::temp_dir().join(format!("fleet-sup-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for repo in names {
            let _ = fs::create_dir_all(root.join("code").join(repo).join(".git"));
        }
        let _ = fs::create_dir_all(root.join("st"));
        let root = fs::canonicalize(&root).unwrap_or(root);
        let state = State::open(&root.join("st")).ok();
        assert!(state.is_some());
        (root, Mutex::new(state.unwrap_or_else(|| unreachable!())))
    }

    fn wingman(root: &Path, state: &Mutex<State>, projects: &[&str]) -> Fleet {
        let fleet = Fleet {
            directory: root.join("wing"),
            projects: projects.iter().map(|p| root.join("code").join(p)).collect(),
        };
        let _ = fs::create_dir_all(&fleet.directory);
        assert!(
            lock(state)
                .write(|w| w.create_fleet(&fleet, || Ok(())))
                .is_ok()
        );
        fleet
    }

    fn write_config(root: &Path, projects: &[&str]) {
        let list: Vec<String> = projects
            .iter()
            .map(|p| format!("{:?}", root.join("code").join(p).display().to_string()))
            .collect();
        let _ = fs::write(
            root.join("wing").join("fleet.toml"),
            format!("projects = [{}]\n", list.join(", ")),
        );
    }

    #[test]
    fn a_refused_start_project_changes_nothing() {
        let (root, state) = tree("refused", &["app", "web", "blog"]);
        let before = wingman(&root, &state, &["app"]);
        write_config(&root, &["app", "web"]);
        let reply = project_on(&state, &root.join("code").join("blog")).ok();
        assert!(
            matches!(reply, Some(Reply::Status(Status::NotServed { .. }))),
            "{reply:?}"
        );
        assert_eq!(lock(&state).fleets().ok(), Some(vec![before]));
        let _ = fs::remove_dir_all(root);
    }
}
