//! Agents' sessions: launched for seats and spawned from roles, through an
//! adapter into tmux, and ended.

use std::path::{Path, PathBuf};

use fleet_core::agent::{self, Verb};
use fleet_core::protocol::{Reply, Response, Status, request, response};
use fleet_core::seats::{Role, SeatKey};
use fleet_core::status::{self, Takes};
use fleet_core::tools::ToolError;
use state::{SessionRow, State, StateError, Step, Write, now_millis};

use crate::names::{self, Named};
use crate::{Error, Supervisor, Tools, adapters};

/// What a launch asks for.
pub(crate) struct Ask {
    /// The adapter, as the request names it: a path, or `<name>@<version>`.
    pub(crate) adapter: String,
    /// Where the agent works.
    pub(crate) directory: PathBuf,
    /// The model to run.
    pub(crate) model: Option<String>,
    /// The first thing the agent is told.
    pub(crate) prompt: Option<String>,
}

/// The seat a session is launched for.
pub(crate) struct ForSeat {
    /// Which seat.
    pub(crate) key: SeatKey,
    /// The label of the role it is launched from.
    pub(crate) role: String,
    /// The digest of the role it is launched from.
    pub(crate) role_digest: String,
    /// Its sessions whose pane ended, forgotten and ended first.
    pub(crate) clear: Vec<String>,
}

/// What a session is launched for.
pub(crate) enum For<'a> {
    /// A seat, which the tending loop keeps filled.
    Seat(&'a ForSeat),
    /// A spawn from role `role` of the fleet in `fleet`.
    Spawn {
        /// The fleet's directory.
        fleet: PathBuf,
        /// The role's label.
        role: String,
        /// The role's digest.
        digest: String,
    },
}

/// How launching a seat's session came out.
#[derive(Debug)]
pub(crate) enum Started {
    /// It runs.
    Launched,
    /// It was refused, for this reason.
    Refused(Status),
    /// By the time it was written, its fleet was off or gone, or its seat
    /// was gone or its role changed: nothing was launched or recorded.
    Unwanted,
}

/// An agent its adapter said how to run, with its row and step, not yet in
/// tmux.
struct Prepared {
    row: SessionRow,
    step: Step,
    launched: agent::Launched,
}

/// Launches `seat`'s session from `ask`: the adapter resolved and asked
/// what to run, without the state lock; then, in one transaction, the seat
/// checked to be still wanted, its `clear`ed sessions forgotten and ended,
/// tmux run, and the row and its steps written, or none of it.
pub(crate) fn start_seat(sup: &Supervisor, ask: &Ask, seat: &ForSeat) -> Result<Started, Error> {
    let tools = &sup.tools;
    let p = match prepare(sup, ask, &For::Seat(seat)) {
        Ok(p) => p,
        Err(status) => return Ok(Started::Refused(status)),
    };
    let written = sup.lock().write(|w| {
        // A fetch or the adapter may have taken long: what was wanted then
        // may not be now.
        if !w.seat_wanted(&seat.key.fleet, &seat.key.seat, &seat.role_digest)? {
            return Ok(false);
        }
        for gone in &seat.clear {
            let step = Step::SessionGone {
                session: gone.clone(),
                fleet: seat.key.fleet.display().to_string(),
                seat: seat.key.seat.clone(),
            };
            w.end_session(gone, &step, || tools.mux.stop(gone))?;
        }
        launch_in(w, tools, &p)?;
        Ok(true)
    });
    Ok(match settle(written, tools, &p.row.id)? {
        Ok(true) => Started::Launched,
        Ok(false) => Started::Unwanted,
        Err(status) => Started::Refused(status),
    })
}

/// Where a spawn runs: the fleet holding the directory it was asked from,
/// the role, and the agent's directory.
struct Spawnable {
    fleet: PathBuf,
    role: Role,
    directory: PathBuf,
}

/// Spawns a session from role `role` of the fleet holding `directory`, in
/// the role's directory or `working_directory`, one of that fleet's
/// projects by its root. The adapter is asked what to run without the
/// state lock; then, in one transaction, the fleet is checked to be still
/// there, tmux runs the agent, and the row and its step are written, or
/// none of it.
pub(crate) fn spawn(sup: &Supervisor, ask: request::Spawn) -> Result<Reply, Error> {
    let request::Spawn::Session {
        directory,
        role,
        working_directory,
    } = ask;
    let found = match spawnable(&sup.lock(), &directory, &role, working_directory.as_deref())? {
        Ok(found) => found,
        Err(status) => return Ok(Reply::Status(status)),
    };
    let ask = Ask {
        adapter: found.role.adapter.clone(),
        directory: found.directory.clone(),
        model: found.role.model.clone(),
        prompt: found.role.prompt.clone(),
    };
    let launch = For::Spawn {
        fleet: found.fleet.clone(),
        role: found.role.label.clone(),
        digest: found.role.digest(),
    };
    let p = match prepare(sup, &ask, &launch) {
        Ok(p) => p,
        Err(status) => return Ok(Reply::Status(status)),
    };
    let written = sup.lock().write(|w| {
        // A fetch or the adapter may have taken long: the fleet may be gone.
        if !w.fleet_registered(&found.fleet)? {
            return Ok(false);
        }
        launch_in(w, &sup.tools, &p)?;
        Ok(true)
    });
    Ok(match settle(written, &sup.tools, &p.row.id)? {
        Ok(true) => Reply::Response(Response::Spawn(response::Spawn::Spawned {
            session: p.row.id,
            fleet: found.fleet,
            role: found.role.label,
            directory: found.directory,
        })),
        Ok(false) => Reply::Status(Status::Session(status::Session::NoFleetHere { directory })),
        Err(status) => Reply::Status(status),
    })
}

/// The fleet holding `directory` (made canonical when it can be), its role
/// `label`, and where the agent works: `working_directory`, made canonical,
/// when it is one of that fleet's projects, else the role's directory.
fn spawnable(
    state: &State,
    directory: &Path,
    label: &str,
    working_directory: Option<&Path>,
) -> Result<Result<Spawnable, Status>, Error> {
    let Some(fleet) = crate::names::here(state, directory)? else {
        return Ok(Err(Status::Session(status::Session::NoFleetHere {
            directory: directory.to_path_buf(),
        })));
    };
    let Some(role) = state.roster(&fleet.directory)?.role(label).cloned() else {
        return Ok(Err(Status::Session(status::Session::NoSuchRole {
            role: label.to_owned(),
            fleet: fleet.directory.clone(),
        })));
    };
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let works = match working_directory.map(canonical) {
        None => role.directory.clone(),
        Some(named) if fleet.projects.contains(&named) => named,
        Some(named) => {
            return Ok(Err(Status::Session(status::Session::NotAProject {
                path: named,
                fleet: fleet.directory.clone(),
            })));
        }
    };
    Ok(Ok(Spawnable {
        fleet: fleet.directory,
        role,
        directory: works,
    }))
}

/// Checks `ask`'s directory, resolves its adapter (fetching it on first
/// use) and asks it what to run, for what `launch` says. No state is
/// touched.
fn prepare(sup: &Supervisor, ask: &Ask, launch: &For<'_>) -> Result<Prepared, Status> {
    if !ask.directory.is_dir() {
        return Err(Status::File(status::File::Missing {
            path: ask.directory.clone(),
        }));
    }
    let program = adapters::resolve(sup, &ask.adapter)?;
    let request = agent::Launch {
        directory: ask.directory.clone(),
        model: ask.model.clone(),
        prompt: ask.prompt.clone(),
    };
    let mut launched = sup
        .tools
        .agents
        .launch(&program, &request)
        .map_err(|fault| {
            Status::Session(status::Session::AdapterFailed {
                adapter: program.clone(),
                verb: Verb::Launch,
                fault,
            })
        })?;
    let id = crate::new_id('s');
    // The agent can name itself as a message's sender.
    launched
        .env
        .insert(fleet_core::messages::SESSION_VAR.to_owned(), id.clone());
    let (fleet, seat, role, digest) = match launch {
        For::Seat(s) => (&s.key.fleet, Some(&s.key.seat), &s.role, &s.role_digest),
        For::Spawn {
            fleet,
            role,
            digest,
        } => (fleet, None, role, digest),
    };
    let row = SessionRow {
        id,
        adapter: program,
        directory: ask.directory.clone(),
        launch_time_millis: now_millis(),
        pane: String::new(),
        fleet: Some(fleet.clone()),
        seat: seat.cloned(),
        role: Some(role.clone()),
        role_digest: Some(digest.clone()),
        state: None,
    };
    let step = Step::SessionLaunched {
        session: row.id.clone(),
        adapter: ask.adapter.clone(),
        program: row.adapter.display().to_string(),
        directory: ask.directory.display().to_string(),
        model: ask.model.clone(),
        prompt: ask.prompt.clone(),
        fleet: Some(fleet.display().to_string()),
        seat: seat.cloned(),
        role: Some(role.clone()),
    };
    Ok(Prepared {
        row,
        step,
        launched,
    })
}

/// Runs `p`'s agent in tmux and writes its row and step, inside the
/// caller's transaction.
fn launch_in(w: &mut Write<'_>, tools: &Tools, p: &Prepared) -> Result<(), StateError> {
    w.launch_session(&p.row, || {
        tools.mux.launch(&p.row.id, &p.row.directory, &p.launched)
    })?;
    w.record(&p.step)
}

/// What a launch's transaction came to: done, refused because tmux failed,
/// or an error, after ending session `id`, which may be running with no
/// record of it.
fn settle<T>(
    written: Result<T, StateError>,
    tools: &Tools,
    id: &str,
) -> Result<Result<T, Status>, Error> {
    match written {
        Ok(done) => Ok(Ok(done)),
        Err(StateError::Tool(e)) => Ok(Err(tool_status(e))),
        Err(e) => {
            let _ = tools.mux.stop(id);
            Err(e.into())
        }
    }
}

/// Ends `name`: a seat of the fleet holding `directory`, its live session,
/// else a session by its id. In one transaction the session is forgotten,
/// its tmux session ended, its step recorded and the messages held for it
/// dropped, or nothing changes; a session already ended changes nothing.
/// The state lock is held from finding the session to ending it.
pub(crate) fn end(sup: &Supervisor, ask: request::End) -> Result<Reply, Error> {
    let request::End::Session { directory, name } = ask;
    let tools = &sup.tools;
    let mut state = sup.lock();
    let row = match endable(&state, tools, &directory, &name)? {
        Ok(row) => row,
        Err(reply) => return Ok(reply),
    };
    let step = Step::SessionEnded {
        session: row.id.clone(),
        fleet: row.fleet.as_ref().map(|f| f.display().to_string()),
        seat: row.seat.clone(),
    };
    // Only a seat still in its fleet's config comes back: an orphan's or a
    // removed fleet's does not.
    let revives = match (&row.fleet, &row.seat) {
        (Some(fleet), Some(seat)) => state.roster(fleet)?.seats.iter().any(|s| s.name == *seat),
        _ => false,
    };
    let written = state.write(|w| w.end_session(&row.id, &step, || tools.mux.stop(&row.id)));
    match written {
        Ok(_) => Ok(Reply::Response(Response::End(response::End::Ended {
            session: row.id,
            seat: row.seat.filter(|_| revives),
        }))),
        Err(StateError::Tool(e)) => Ok(Reply::Status(tool_status(e))),
        Err(e) => Err(e.into()),
    }
}

/// The session `name` names from `directory`: a seat of the fleet here,
/// its session whose pane is alive; else the session with that id. What
/// to answer instead when there is none to end.
fn endable(
    state: &State,
    tools: &Tools,
    directory: &Path,
    name: &str,
) -> Result<Result<SessionRow, Reply>, Error> {
    let resolved = names::resolve(state, directory, name, Takes::SeatOrSession)?;
    let key = match resolved.named {
        Some(Named::Seat(key)) => key,
        Some(Named::Session(row)) => return Ok(Ok(row)),
        Some(Named::Ended(session)) => {
            return Ok(Err(Reply::Response(Response::End(
                response::End::AlreadyEnded { session },
            ))));
        }
        Some(Named::Message(_)) | None => {
            return Ok(Err(Reply::Status(resolved.unknown().into())));
        }
    };
    let seats: Vec<SessionRow> = state
        .sessions()?
        .into_iter()
        .filter(|r| names::of_seat(r, &key.fleet, &key.seat))
        .collect();
    let alive = match crate::observe::alive(tools, &seats) {
        Ok(alive) => alive,
        Err(e) => return Ok(Err(Reply::Status(tool_status(e)))),
    };
    Ok(seats
        .into_iter()
        .find(|r| alive.get(&r.id).copied().unwrap_or(false))
        .ok_or(Reply::Status(Status::Session(
            status::Session::SeatNotRunning {
                seat: key.seat,
                fleet: key.fleet,
            },
        ))))
}

/// What tmux failing comes to, as the facts the CLI words.
pub(crate) fn tool_status(error: ToolError) -> Status {
    match error {
        ToolError::Failed { command, said } => {
            Status::Session(status::Session::MultiplexerFailed { command, said })
        }
        ToolError::CouldNotRun { error, .. } => {
            Status::Session(status::Session::MultiplexerCouldNotRun {
                error: error.to_string(),
            })
        }
        ToolError::File { path, .. } => Status::File(status::File::Missing { path }),
        ToolError::TooLong { bytes, limit } => {
            Status::Session(status::Session::LaunchTooLong { bytes, limit })
        }
        other => Status::Session(status::Session::MultiplexerFailed {
            command: "tmux".into(),
            said: format!("{other:?}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_launch_too_long_for_tmux_is_refused_as_such() {
        assert_eq!(
            tool_status(ToolError::TooLong {
                bytes: 20_000,
                limit: 16_000
            }),
            Status::Session(status::Session::LaunchTooLong {
                bytes: 20_000,
                limit: 16_000
            })
        );
    }
}
