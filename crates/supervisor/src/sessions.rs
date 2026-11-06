//! Agents' sessions: launched for seats and spawned from roles, through an
//! adapter into tmux, and ended.

use std::path::{Path, PathBuf};

use fleet_core::agent::{self, Verb};
use fleet_core::protocol::{Reply, Response, Status, request, response};
use fleet_core::seats::{Role, SeatKey};
use fleet_core::status::{self, Takes};
use fleet_core::tools::ToolError;
use state::{Launched, Life, Named, SessionRow, State, StateError, Write, now_millis};

use crate::names;
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
    /// Its newest session, gone, whose tmux session is stopped first.
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

/// An agent its adapter said how to run, with its row and what it was
/// asked, not yet in tmux.
struct Prepared {
    row: SessionRow,
    asked: Launched,
    launched: agent::Launched,
}

/// Launches `seat`'s session from `ask`: the adapter resolved and asked
/// what to run, without the state lock; then, in one transaction, the seat
/// checked to be still wanted, the tmux session of its `clear`ed session
/// stopped, tmux run, and the row and its steps written, or none of it.
pub(crate) fn start_seat(sup: &Supervisor, ask: &Ask, seat: &ForSeat) -> Result<Started, Error> {
    let tools = &sup.tools;
    let p = match prepare(sup, ask, &For::Seat(seat)) {
        Ok(p) => p,
        Err(status) => return Ok(Started::Refused(status)),
    };
    let written = sup.lock().write(|w| {
        // A fetch or the adapter may have taken long: what was wanted then
        // may not be now.
        if !w.seat_wanted(&seat.key, Some(&seat.role_digest))? {
            return Ok(false);
        }
        for gone in &seat.clear {
            tools.mux.stop(gone).map_err(StateError::Tool)?;
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
        life: Life::Live,
    };
    let asked = Launched {
        adapter: ask.adapter.clone(),
        model: ask.model.clone(),
        prompt: ask.prompt.clone(),
    };
    Ok(Prepared {
        row,
        asked,
        launched,
    })
}

/// Runs `p`'s agent in tmux and writes its row and step, inside the
/// caller's transaction.
fn launch_in(w: &mut Write<'_>, tools: &Tools, p: &Prepared) -> Result<(), StateError> {
    w.launch_session(&p.row, &p.asked, || {
        tools.mux.launch(&p.row.id, &p.row.directory, &p.launched)
    })
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
/// else a session by its id, live or gone. In one transaction the session
/// is ended, its tmux session stopped, its step recorded and the messages
/// held for it dropped, or nothing changes; a session already ended
/// changes nothing.
/// The state lock is held from finding the session to ending it.
pub(crate) fn end(sup: &Supervisor, ask: request::End) -> Result<Reply, Error> {
    let request::End::Session { directory, name } = ask;
    let tools = &sup.tools;
    let mut state = sup.lock();
    let row = match endable(&state, &directory, &name)? {
        Ok(row) => row,
        Err(reply) => return Ok(reply),
    };
    let written = state.write(|w| {
        // Only a seat whose live session ended, and that is still wanted,
        // comes back: an old gone session's, an orphan's, a removed
        // fleet's or one whose fleet is off does not.
        let revives = match (&row.fleet, &row.seat) {
            (Some(fleet), Some(seat)) if row.life == Life::Live => {
                let key = SeatKey {
                    fleet: fleet.clone(),
                    seat: seat.clone(),
                };
                w.seat_wanted(&key, None)?
            }
            _ => false,
        };
        w.end_session(&row.id, || tools.mux.stop(&row.id))?;
        Ok(revives)
    });
    match written {
        Ok(revives) => Ok(Reply::Response(Response::End(response::End::Ended {
            session: row.id,
            seat: row.seat.filter(|_| revives),
        }))),
        Err(StateError::Tool(e)) => Ok(Reply::Status(tool_status(e))),
        Err(e) => Err(e.into()),
    }
}

/// The session `name` names from `directory`: a seat of the fleet here,
/// its live session; else the session with that id, unless it is ended.
/// What to answer instead when there is none to end.
fn endable(
    state: &State,
    directory: &Path,
    name: &str,
) -> Result<Result<SessionRow, Reply>, Error> {
    let resolved = names::resolve(state, directory, name, Takes::SeatOrSession)?;
    let key = match resolved.named {
        Some(Named::Seat(key)) => key,
        Some(Named::Session(row)) if row.life == Life::Ended => {
            return Ok(Err(Reply::Response(Response::End(
                response::End::AlreadyEnded { session: row.id },
            ))));
        }
        Some(Named::Session(row)) => return Ok(Ok(row)),
        Some(Named::Message(_)) | None => {
            return Ok(Err(Reply::Status(resolved.unknown().into())));
        }
    };
    let live = state
        .sessions()?
        .into_iter()
        .find(|r| r.life == Life::Live && names::of_seat(r, &key.fleet, &key.seat));
    Ok(live.ok_or(Reply::Status(Status::Session(
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
