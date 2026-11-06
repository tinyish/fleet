//! Turning projects on and off, re-reading each fleet's config on the way.
//! On and off belong to each project, not to its fleet.

use std::fs;
use std::path::{Path, PathBuf};

use fleet_core::fleets::{self, Fleet, conflicts};
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::seats::Roster;
use fleet_core::status;
use state::{State, StateError, Write};

use crate::{Error, Supervisor, canonical, repo_root_of};

/// The fleet as its config now describes it, and its roles and seats:
/// read, parsed and resolved, but not yet checked against other fleets.
fn read_resolved(fleet: &Fleet) -> Result<(Fleet, Roster), Status> {
    let (projects, roster) = fleets::read_fleet(
        &fleet.directory,
        |p| fs::read_to_string(p),
        |p| fs::canonicalize(p),
        repo_root_of,
    )?;
    Ok((
        Fleet {
            directory: fleet.directory.clone(),
            projects,
        },
        roster,
    ))
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
    /// Each fleet that read: its roles and seats.
    rosters: Vec<(PathBuf, Roster)>,
    /// Fleets whose directory no longer exists: they are forgotten. One that
    /// cannot be looked at (permission denied) is not gone; it is broken.
    gone: Vec<Fleet>,
}

/// Re-reads every fleet's config, in two phases: first every config is
/// read and resolved, then claims are checked against what every fleet now
/// wants, so a project moved between fleets in one edit is no conflict.
/// Nothing is written: `apply` writes it.
fn reread_all(state: &State) -> Result<Reread, Error> {
    let registered = state.fleets()?;
    let mut read = Vec::new();
    let mut broken = Vec::new();
    let mut gone = Vec::new();
    for fleet in registered {
        if matches!(fleet.directory.try_exists(), Ok(false)) {
            gone.push(fleet);
            continue;
        }
        match read_resolved(&fleet) {
            Ok((wanted, roster)) => read.push((fleet, wanted, roster)),
            Err(why) => broken.push((fleet, why)),
        }
    }
    let claims: Vec<Fleet> = read
        .iter()
        .map(|(_, wanted, _)| wanted.clone())
        .chain(broken.iter().map(|(stored, _)| stored.clone()))
        .collect();
    let clashes = conflicts(&claims);
    let mut ok = Vec::new();
    let mut changes = Vec::new();
    let mut rosters = Vec::new();
    for (stored, wanted, roster) in read {
        if let Some((_, why)) = clashes.iter().find(|(dir, _)| *dir == stored.directory) {
            broken.push((stored, why.clone()));
        } else {
            let change = fleets::sync(&stored.projects, &wanted.projects);
            changes.push((stored.directory.clone(), change));
            rosters.push((stored.directory.clone(), roster));
            ok.push(wanted);
        }
    }
    Ok(Reread {
        ok,
        broken,
        changes,
        rosters,
        gone,
    })
}

impl Reread {
    /// The fleets the re-read forgets, for the answer.
    fn forgotten(&self) -> Vec<PathBuf> {
        self.gone.iter().map(|f| f.directory.clone()).collect()
    }

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

    /// Writes the re-read, each write recording its steps, inside the
    /// caller's transaction. A project only added comes in off; a moved one
    /// keeps its on or off. A fleet that read has its roles and seats
    /// replaced; a broken one keeps the ones it had.
    fn apply(&self, w: &mut Write<'_>) -> Result<(), StateError> {
        w.apply_reread(&self.changes)?;
        for (fleet, roster) in &self.rosters {
            w.set_roster(fleet, roster)?;
        }
        for (fleet, why) in &self.broken {
            w.config_broken(&fleet.directory, why)?;
        }
        for fleet in &self.gone {
            w.forget_fleet(&fleet.directory)?;
        }
        Ok(())
    }
}

/// Re-reads every fleet's config and turns every project on. A fleet whose
/// config does not read, or claims another fleet's project, stays off and is
/// named in the answer.
pub(crate) fn all_on(sup: &Supervisor) -> Result<Reply, Error> {
    let mut state = sup.lock();
    let reread = reread_all(&state)?;
    let on: Vec<PathBuf> = reread.ok.iter().map(|f| f.directory.clone()).collect();
    let off: Vec<PathBuf> = reread
        .broken
        .iter()
        .map(|(f, _)| f.directory.clone())
        .collect();
    state.write(|w| {
        reread.apply(w)?;
        w.all_on(&on, &off)
    })?;
    let forgotten = reread.forgotten();
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
        forgotten,
    };
    Ok(Reply::Response(Response::Start(response::Start::AllOn(
        all_on,
    ))))
}

/// Turns every project off.
pub(crate) fn all_off(sup: &Supervisor) -> Result<Reply, Error> {
    let mut state = sup.lock();
    state.write(|w: &mut Write<'_>| w.all_off())?;
    Ok(Reply::Response(Response::Stop(response::Stop::AllOff)))
}

/// Why no fleet's project is `path`: it is inside one, or nobody serves it.
fn not_served(fleets: &[Fleet], path: &Path) -> Status {
    let inside = fleets
        .iter()
        .flat_map(|f| f.projects.iter())
        .find(|p| path.starts_with(p));
    match inside {
        Some(project) => Status::Project(status::Project::InsideProject {
            path: path.to_path_buf(),
            project: project.clone(),
        }),
        None => Status::Project(status::Project::NotServed {
            path: path.to_path_buf(),
        }),
    }
}

/// Turns one project on, after re-reading every fleet's config so a project
/// just added to one is found. No other project's on or off changes, even
/// when its fleet's config does not read. A refusal changes nothing: the
/// re-read is applied only when the project comes on.
pub(crate) fn project_on(sup: &Supervisor, project: &Path) -> Result<Reply, Error> {
    let project = match fs::canonicalize(project) {
        Ok(p) => p,
        Err(e) => return Ok(Reply::Status(Status::unusable(project, &e))),
    };
    let mut state = sup.lock();
    let reread = reread_all(&state)?;
    if let Some((fleet, why)) = reread
        .broken
        .iter()
        .find(|(f, _)| f.projects.contains(&project))
    {
        return Ok(Reply::Status(Status::Project(
            status::Project::CannotTurnOn {
                project,
                fleet: fleet.directory.clone(),
                why: Box::new(why.clone()),
            },
        )));
    }
    if !reread.ok.iter().any(|f| f.projects.contains(&project)) {
        let dropped = reread
            .changes
            .iter()
            .find(|(_, c)| c.removed.contains(&project));
        if let Some((fleet, _)) = dropped {
            return Ok(Reply::Status(Status::Project(
                status::Project::NoLongerServed {
                    project,
                    fleet: fleet.clone(),
                },
            )));
        }
        let all: Vec<Fleet> = reread
            .ok
            .iter()
            .cloned()
            .chain(reread.broken.iter().map(|(f, _)| f.clone()))
            .collect();
        return Ok(Reply::Status(not_served(&all, &project)));
    }
    state.write(|w| {
        reread.apply(w)?;
        w.set_active(&project, true)
    })?;
    Ok(Reply::Response(Response::Start(
        response::Start::ProjectOn(response::ProjectOn {
            project,
            picked_up: reread.picked_up(),
            forgotten: reread.forgotten(),
        }),
    )))
}

/// Turns one project off; the rest of its fleet keeps its own on or off. A
/// project whose repository is gone is still found by the path given.
pub(crate) fn project_off(sup: &Supervisor, project: &Path) -> Result<Reply, Error> {
    let project = canonical(project);
    let mut state = sup.lock();
    let registered = state.fleets()?;
    let Some(fleet) = registered
        .iter()
        .find(|f| f.projects.contains(&project))
        .cloned()
    else {
        return Ok(Reply::Status(not_served(&registered, &project)));
    };
    state.write(|w| w.set_active(&project, false))?;
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
