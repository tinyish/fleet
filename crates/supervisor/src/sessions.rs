//! Agents' sessions: launched for seats and spawned from roles, through an
//! adapter into tmux, and ended.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::agent::{self, Verb};
use fleet_core::fleets;
use fleet_core::messages::Dropped;
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::seats::{Role, SeatKey};
use fleet_core::status;
use fleet_core::tools::ToolError;
use state::{SessionRow, State, StateError, Step, Write, now_millis};

use crate::{Error, Tools, adapters, lock};

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
pub(crate) fn start_seat(
    state: &Mutex<State>,
    tools: &Tools,
    store: &adapters::Store,
    ask: &Ask,
    seat: &ForSeat,
) -> Result<Started, Error> {
    let p = match prepare(tools, store, ask, &For::Seat(seat)) {
        Ok(p) => p,
        Err(status) => return Ok(Started::Refused(status)),
    };
    let written = lock(state).write(|w| {
        // A fetch or the adapter may have taken long: what was wanted then
        // may not be now.
        if !w.seat_wanted(&seat.key.fleet, &seat.key.seat, &seat.role_digest)? {
            return Ok(false);
        }
        for gone in &seat.clear {
            if w.end_session(gone, || tools.mux.stop(gone))?.is_some() {
                w.record(&Step::SessionGone {
                    session: gone.clone(),
                    fleet: seat.key.fleet.display().to_string(),
                    seat: seat.key.seat.clone(),
                })?;
                let ids = w.drop_for_session(gone)?;
                crate::messages::dropped(w, &ids, Dropped::SessionEnded)?;
            }
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
pub(crate) fn spawn(
    state: &Mutex<State>,
    tools: &Tools,
    store: &adapters::Store,
    directory: &Path,
    role: &str,
    working_directory: Option<&Path>,
) -> Result<Reply, Error> {
    let found = match spawnable(&lock(state), directory, role, working_directory)? {
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
    let p = match prepare(tools, store, &ask, &launch) {
        Ok(p) => p,
        Err(status) => return Ok(Reply::Status(status)),
    };
    let written = lock(state).write(|w| {
        // A fetch or the adapter may have taken long: the fleet may be gone.
        if !w.fleet_registered(&found.fleet)? {
            return Ok(false);
        }
        launch_in(w, tools, &p)?;
        Ok(true)
    });
    Ok(match settle(written, tools, &p.row.id)? {
        Ok(true) => Reply::Response(Response::Spawn(response::Spawn::Spawned {
            session: p.row.id,
            fleet: found.fleet,
            role: found.role.label,
            directory: found.directory,
        })),
        Ok(false) => Reply::Status(Status::Session(status::Session::NoFleetHere {
            directory: directory.to_path_buf(),
        })),
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
    let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let directory = canonical(directory);
    let fleets = state.fleets()?;
    let Some(fleet) = fleets::holding(&fleets, &directory) else {
        return Ok(Err(Status::Session(status::Session::NoFleetHere {
            directory,
        })));
    };
    let Some(role) = state.roster(&fleet.directory)?.role(label).cloned() else {
        return Ok(Err(Status::Session(status::Session::NoSuchRole {
            role: label.to_owned(),
            fleet: fleet.directory.clone(),
        })));
    };
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
        fleet: fleet.directory.clone(),
        role,
        directory: works,
    }))
}

/// Checks `ask`'s directory, resolves its adapter (fetching it on first
/// use) and asks it what to run, for what `launch` says. No state is
/// touched.
fn prepare(
    tools: &Tools,
    store: &adapters::Store,
    ask: &Ask,
    launch: &For<'_>,
) -> Result<Prepared, Status> {
    if !ask.directory.is_dir() {
        return Err(Status::File(status::File::Missing {
            path: ask.directory.clone(),
        }));
    }
    let program = adapters::resolve(store, tools, &ask.adapter)?;
    let request = agent::Launch {
        directory: ask.directory.clone(),
        model: ask.model.clone(),
        prompt: ask.prompt.clone(),
    };
    let mut launched = tools.agents.launch(&program, &request).map_err(|fault| {
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
/// dropped, or nothing changes. The state lock is held from finding the
/// session to ending it.
pub(crate) fn end(
    state: &Mutex<State>,
    tools: &Tools,
    directory: &Path,
    name: &str,
) -> Result<Reply, Error> {
    let mut state = lock(state);
    let row = match endable(&state, tools, directory, name)? {
        Ok(row) => row,
        Err(status) => return Ok(Reply::Status(status)),
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
    let written = state.write(|w| {
        if w.end_session(&row.id, || tools.mux.stop(&row.id))?
            .is_some()
        {
            w.record(&step)?;
            let ids = w.drop_for_session(&row.id)?;
            crate::messages::dropped(w, &ids, Dropped::SessionEnded)?;
        }
        Ok(())
    });
    match written {
        Ok(()) => Ok(Reply::Response(Response::End(response::End::Ended {
            session: row.id,
            seat: row.seat.filter(|_| revives),
        }))),
        Err(StateError::Tool(e)) => Ok(Reply::Status(tool_status(e))),
        Err(e) => Err(e.into()),
    }
}

/// The session `name` names from `directory` (made canonical when it can
/// be): a seat of the fleet holding it, its session whose pane is alive;
/// else the session with that id.
fn endable(
    state: &State,
    tools: &Tools,
    directory: &Path,
    name: &str,
) -> Result<Result<SessionRow, Status>, Error> {
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let fleets = state.fleets()?;
    let here = fleets::holding(&fleets, &directory).map(|f| f.directory.clone());
    let rows = state.sessions()?;
    if let Some(fleet) = &here
        && state.roster(fleet)?.seats.iter().any(|s| s.name == name)
    {
        let seats: Vec<SessionRow> = rows
            .into_iter()
            .filter(|r| r.fleet.as_ref() == Some(fleet) && r.seat.as_deref() == Some(name))
            .collect();
        let alive = match crate::observe::alive(tools, &seats) {
            Ok(alive) => alive,
            Err(e) => return Ok(Err(tool_status(e))),
        };
        return Ok(seats
            .into_iter()
            .find(|r| alive.get(&r.id).copied().unwrap_or(false))
            .ok_or_else(|| {
                Status::Session(status::Session::SeatNotRunning {
                    seat: name.to_owned(),
                    fleet: fleet.clone(),
                })
            }));
    }
    Ok(rows.into_iter().find(|r| r.id == name).ok_or_else(|| {
        Status::Session(status::Session::NoSuchSeatOrSession {
            name: name.to_owned(),
            fleet: here,
        })
    }))
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
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, PoisonError};

    use fleet_core::agent::{self, AdapterFault, SessionState, Verb};
    use fleet_core::protocol::{Reply, Response, Status, response};
    use fleet_core::tools::{AgentAdapters, Pane};

    use crate::Tools;
    use crate::testing::{FakeAgents, FakeMux, FakeNet, Shared, SharedMux, code, crew};

    fn tools(mux: FakeMux, adapters: FakeAgents) -> Tools {
        Tools {
            mux: Box::new(mux),
            agents: Box::new(adapters),
            net: Box::new(FakeNet::default()),
        }
    }

    /// The tree's adapter store, fetching from a base no test serves.
    fn store(root: &Path) -> crate::adapters::Store {
        crate::adapters::Store::new(&root.join("st"), Some("https://r".into()))
    }

    /// Spawns role `role` from `from`, with the tree's store.
    fn spawn_in(
        state: &Mutex<State>,
        tools: &Tools,
        root: &Path,
        from: &Path,
        role: &str,
    ) -> Reply {
        spawn(state, tools, &store(root), from, role, None).unwrap_or_else(|e| {
            Reply::Status(Status::Malformed {
                error: format!("{e:?}"),
            })
        })
    }

    /// The id a spawn answered with; empty for any other reply.
    fn spawned_id(reply: &Reply) -> String {
        match reply {
            Reply::Response(Response::Spawn(response::Spawn::Spawned { session, .. })) => {
                session.clone()
            }
            _ => String::new(),
        }
    }

    /// What `fleet status` shows from `from`: the spawned sessions of its
    /// fleet, and the faults.
    fn spawned_of(
        state: &Mutex<State>,
        tools: &Tools,
        from: &Path,
    ) -> (Vec<response::Listed>, Vec<response::Fault>) {
        match crate::overview::overview(state, tools, &crate::tending::Tending::new(), from) {
            Ok(Reply::Response(Response::Overview(o))) => (
                o.fleets
                    .into_iter()
                    .flat_map(|f| f.spawned)
                    .map(|s| s.session)
                    .collect(),
                o.faults,
            ),
            _ => (vec![], vec![]),
        }
    }

    /// Tools whose tmux a test keeps a handle on.
    fn watched(mux: &Arc<FakeMux>) -> Tools {
        Tools {
            mux: Box::new(SharedMux(mux.clone())),
            agents: Box::new(FakeAgents::default()),
            net: Box::new(FakeNet::default()),
        }
    }

    fn kill_all(mux: &FakeMux) {
        for p in mux
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
        {
            p.dead = true;
        }
    }

    #[test]
    fn a_spawned_session_lists_with_the_adapters_reading() {
        let (root, state, wing) = crew("sess-list", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let id = spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker"));
        assert!(id.starts_with('s') && id.len() == 11, "{id}");
        let (sessions, faults) = spawned_of(&state, &tools, &wing);
        assert_eq!(faults, []);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session, id);
        assert_eq!(sessions[0].adapter, PathBuf::from("/a/claude"));
        assert_eq!(sessions[0].directory, wing);
        assert_eq!(sessions[0].state, SessionState::Idle);
    }

    #[test]
    fn a_session_whose_pane_ended_is_gone_without_asking_the_adapter() {
        let (root, state, wing) = crew("sess-gone", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        assert_ne!(
            spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker")),
            ""
        );
        // The pane's agent exited.
        let panes = tools.mux.panes().unwrap_or_default();
        let dead: Vec<Pane> = panes
            .into_iter()
            .map(|p| Pane { dead: true, ..p })
            .collect();
        let tools = Tools {
            mux: Box::new(FakeMux {
                panes: Mutex::new(dead),
                ..FakeMux::default()
            }),
            // Asked at all, it would fail, and say so in `faults`.
            agents: Box::new(FakeAgents {
                read_fault: Some(AdapterFault::TimedOut { after_millis: 1 }),
                ..FakeAgents::default()
            }),
            net: Box::new(FakeNet::default()),
        };
        let (sessions, faults) = spawned_of(&state, &tools, &wing);
        assert_eq!(sessions[0].state, SessionState::Gone);
        assert_eq!(faults, []);
    }

    #[test]
    fn an_adapter_fault_makes_its_sessions_unknown_and_says_why() {
        let (root, state, wing) = crew("sess-fault", &[("worker", "/a/claude")]);
        let fault = AdapterFault::TimedOut {
            after_millis: 20_000,
        };
        let tools = tools(
            FakeMux::default(),
            FakeAgents {
                read_fault: Some(fault.clone()),
                ..FakeAgents::default()
            },
        );
        assert_ne!(
            spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker")),
            ""
        );
        let (sessions, faults) = spawned_of(&state, &tools, &wing);
        assert_eq!(sessions[0].state, SessionState::Unknown);
        assert_eq!(
            faults,
            [response::Fault {
                adapter: PathBuf::from("/a/claude"),
                fault
            }]
        );
    }

    #[test]
    fn a_failed_tmux_launch_leaves_no_session_and_no_record() {
        let (root, state, wing) = crew("sess-tmux-fails", &[("worker", "/a/claude")]);
        let tools = tools(
            FakeMux {
                fail_launch: true,
                ..FakeMux::default()
            },
            FakeAgents::default(),
        );
        let reply = spawn_in(&state, &tools, &root, &wing, "worker");
        assert!(
            matches!(
                reply,
                Reply::Status(Status::Session(status::Session::MultiplexerFailed { .. }))
            ),
            "{reply:?}"
        );
        assert_eq!(crate::lock(&state).sessions().ok(), Some(vec![]));
    }

    #[test]
    fn a_tmux_that_is_not_installed_is_said_so() {
        let (root, state, wing) = crew("sess-no-tmux", &[("worker", "/a/claude")]);
        let tools = tools(
            FakeMux {
                missing: true,
                ..FakeMux::default()
            },
            FakeAgents::default(),
        );
        assert_eq!(
            spawn_in(&state, &tools, &root, &wing, "worker"),
            Reply::Status(Status::Session(status::Session::MultiplexerCouldNotRun {
                error: std::io::Error::from(std::io::ErrorKind::NotFound).to_string(),
            }))
        );
        assert_eq!(crate::lock(&state).sessions().ok(), Some(vec![]));
    }

    #[test]
    fn a_missing_directory_is_refused_before_the_adapter_runs() {
        let (root, state, wing) = crew("sess-missing", &[("worker", "/a/claude")]);
        let nope = root.join("nope");
        let mut roster = crate::lock(&state).roster(&wing).unwrap_or_default();
        roster.roles[0].directory = nope.clone();
        assert!(
            crate::lock(&state)
                .write(|w| w.set_roster(&wing, &roster))
                .is_ok()
        );
        let agents = Arc::new(FakeAgents::default());
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(Shared(agents.clone())),
            net: Box::new(FakeNet::default()),
        };
        assert_eq!(
            spawn_in(&state, &tools, &root, &wing, "worker"),
            Reply::Status(Status::File(status::File::Missing { path: nope }))
        );
        assert!(
            agents
                .launches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
        );
    }

    #[test]
    fn an_adapter_that_cannot_launch_is_refused_with_its_fault() {
        let (root, state, wing) = crew("sess-launch-fault", &[("worker", "/a/claude")]);
        let fault = AdapterFault::Exited {
            code: Some(1),
            said: "claude: not found".into(),
        };
        let tools = tools(
            FakeMux::default(),
            FakeAgents {
                launch_fault: Some(fault.clone()),
                ..FakeAgents::default()
            },
        );
        assert_eq!(
            spawn_in(&state, &tools, &root, &wing, "worker"),
            Reply::Status(Status::Session(status::Session::AdapterFailed {
                adapter: PathBuf::from("/a/claude"),
                verb: Verb::Launch,
                fault
            }))
        );
    }

    /// What ending `name` from `from` answered.
    fn end_in(state: &Mutex<State>, tools: &Tools, from: &Path, name: &str) -> Reply {
        end(state, tools, from, name).unwrap_or_else(|e| {
            Reply::Status(Status::Malformed {
                error: format!("{e:?}"),
            })
        })
    }

    #[test]
    fn a_spawned_session_ends_by_its_id_and_a_second_end_finds_nobody() {
        let (root, state, wing) = crew("end-spawned", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let id = spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker"));
        assert_eq!(
            end_in(&state, &tools, &root, &id),
            Reply::Response(Response::End(response::End::Ended {
                session: id.clone(),
                seat: None
            }))
        );
        assert_eq!(tools.mux.panes().ok(), Some(vec![]));
        assert_eq!(crate::lock(&state).sessions().ok(), Some(vec![]));
        let steps = crate::lock(&state).steps().unwrap_or_default();
        assert_eq!(steps.last().map(String::as_str), Some("SESSION_ENDED"));
        assert_eq!(
            end_in(&state, &tools, &wing, &id),
            Reply::Status(Status::Session(status::Session::NoSuchSeatOrSession {
                name: id,
                fleet: Some(wing.clone()),
            }))
        );
    }

    #[test]
    fn a_gone_session_ends_and_is_cleared() {
        let (root, state, wing) = crew("end-gone", &[("worker", "/a/claude")]);
        let mux = Arc::new(FakeMux::default());
        let tools = watched(&mux);
        let id = spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker"));
        kill_all(&mux);
        assert!(matches!(
            end_in(&state, &tools, &wing, &id),
            Reply::Response(Response::End(_))
        ));
        assert_eq!(spawned_of(&state, &tools, &wing).0, []);
    }

    #[test]
    fn ending_a_seat_revives_it_fresh_without_backing_off() {
        let (root, state, fleet) = crate::testing::with_seat("end-seat");
        let app = fleet.directory;
        let mux = Arc::new(FakeMux::default());
        let tools = watched(&mux);
        let tending = crate::tending::Tending::new();
        crate::tending::tick(&state, &tools, &store(&root), &tending, 1_000);
        for (n, now) in [(1_i64, 2_000), (2, 3_000)] {
            let first = crate::lock(&state)
                .sessions()
                .unwrap_or_default()
                .remove(0)
                .id;
            assert_eq!(
                end_in(&state, &tools, &app, "scribe"),
                Reply::Response(Response::End(response::End::Ended {
                    session: first.clone(),
                    seat: Some("scribe".into()),
                })),
                "end {n}"
            );
            crate::tending::tick(&state, &tools, &store(&root), &tending, now);
            let rows = crate::lock(&state).sessions().unwrap_or_default();
            assert_eq!(rows.len(), 1, "end {n}");
            assert_ne!(rows[0].id, first, "end {n}: a fresh session");
            assert!(tending.down().is_empty(), "end {n}: no backoff");
        }
    }

    #[test]
    fn ending_a_seat_session_nothing_revives_names_no_seat() {
        let changes: [(&str, Change); 2] = [
            ("orphaned", |w, f| {
                w.set_roster(f, &fleet_core::seats::Roster::default())
            }),
            ("loose", |w, f| w.remove_fleet(f)),
        ];
        for (what, change) in changes {
            let (root, state, fleet) =
                crate::testing::with_seat(&format!("end-unrevived-{}", what.len()));
            let tools = watched(&Arc::new(FakeMux::default()));
            crate::tending::tick(
                &state,
                &tools,
                &store(&root),
                &crate::tending::Tending::new(),
                1_000,
            );
            let id = crate::lock(&state)
                .sessions()
                .unwrap_or_default()
                .remove(0)
                .id;
            assert!(
                crate::lock(&state)
                    .write(|w| change(w, &fleet.directory))
                    .is_ok()
            );
            assert_eq!(
                end_in(&state, &tools, &root, &id),
                Reply::Response(Response::End(response::End::Ended {
                    session: id.clone(),
                    seat: None,
                })),
                "{what}"
            );
        }
    }

    #[test]
    fn a_seat_with_no_live_session_is_refused() {
        let (_root, state, fleet) = crate::testing::with_seat("end-not-running");
        let tools = tools(FakeMux::default(), FakeAgents::default());
        assert_eq!(
            end_in(&state, &tools, &fleet.directory, "scribe"),
            Reply::Status(Status::Session(status::Session::SeatNotRunning {
                seat: "scribe".into(),
                fleet: fleet.directory.clone(),
            }))
        );
    }

    #[test]
    fn nobody_by_that_name_from_outside_every_fleet_names_no_fleet() {
        let (root, state, _wing) = crew("end-nobody", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        assert_eq!(
            end_in(&state, &tools, &root, "s0"),
            Reply::Status(Status::Session(status::Session::NoSuchSeatOrSession {
                name: "s0".into(),
                fleet: None,
            }))
        );
    }

    #[test]
    fn a_tmux_that_fails_to_end_changes_nothing() {
        let (root, state, wing) = crew("end-tmux", &[("worker", "/a/claude")]);
        let mux = Arc::new(FakeMux::default());
        let id = spawned_id(&spawn_in(&state, &watched(&mux), &root, &wing, "worker"));
        let panes = mux
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let failing = tools(
            FakeMux {
                panes: Mutex::new(panes),
                stop_fails: Some("no"),
                ..FakeMux::default()
            },
            FakeAgents::default(),
        );
        assert!(matches!(
            end_in(&state, &failing, &wing, &id),
            Reply::Status(Status::Session(status::Session::MultiplexerFailed { .. }))
        ));
        assert_eq!(crate::lock(&state).sessions().unwrap_or_default().len(), 1);
        let steps = crate::lock(&state).steps().unwrap_or_default();
        assert!(!steps.iter().any(|s| s == "SESSION_ENDED"), "{steps:?}");
    }

    #[test]
    fn a_named_adapter_is_fetched_then_launched() {
        let (root, state, wing) = crew("sess-named", &[("worker", "claude-code@2.1.285")]);
        let adapters = Arc::new(FakeAgents::default());
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(Shared(adapters.clone())),
            net: Box::new(FakeNet {
                files: crate::testing::good_release("https://r"),
                ..FakeNet::default()
            }),
        };
        let reply = spawn_in(&state, &tools, &root, &wing, "worker");
        assert_ne!(spawned_id(&reply), "", "{reply:?}");
        let kept = root.join("st/adapters/claude-code/2.1.285/fleet-claude-code");
        let launches = adapters
            .launches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(launches, vec![kept.clone()]);
        let (sessions, _) = spawned_of(&state, &tools, &wing);
        assert_eq!(
            sessions
                .iter()
                .map(|s| s.adapter.clone())
                .collect::<Vec<_>>(),
            vec![kept]
        );
    }

    #[test]
    fn a_bare_name_launches_nothing() {
        // State holds the roster as given: a config with a bare name would
        // not read, but resolving refuses it here too.
        let (root, state, wing) = crew("sess-bare", &[("worker", "claude-code")]);
        let adapters = Arc::new(FakeAgents::default());
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(Shared(adapters.clone())),
            net: Box::new(FakeNet::default()),
        };
        assert_eq!(
            spawn_in(&state, &tools, &root, &wing, "worker"),
            Reply::Status(Status::Adapter(status::Adapter::AdapterUnpinned {
                name: "claude-code".into(),
                latest: None
            }))
        );
        assert!(
            adapters
                .launches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
        );
    }

    #[test]
    fn sessions_of_two_adapters_are_read_one_call_each() {
        let (root, state, wing) = crew("sess-two", &[("one", "/a/one"), ("two", "/a/two")]);
        let adapters = Arc::new(FakeAgents::default());
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(Shared(adapters.clone())),
            net: Box::new(FakeNet::default()),
        };
        for role in ["one", "two", "one"] {
            assert_ne!(
                spawned_id(&spawn_in(&state, &tools, &root, &wing, role)),
                ""
            );
        }
        let _ = spawned_of(&state, &tools, &wing);
        let mut reads: Vec<(PathBuf, usize)> = adapters
            .reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(adapter, asked)| (adapter.clone(), asked.len()))
            .collect();
        reads.sort();
        assert_eq!(
            reads,
            [(PathBuf::from("/a/one"), 2), (PathBuf::from("/a/two"), 1)]
        );
    }

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

    #[test]
    fn a_spawned_agent_is_told_its_session_id() {
        let (root, state, wing) = crew("sess-env", &[("worker", "/a/claude")]);
        let mux = Arc::new(FakeMux::default());
        let tools = watched(&mux);
        let id = spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker"));
        let env = mux
            .launched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .first()
            .map(|l| l.env.clone())
            .unwrap_or_default();
        assert!(id.starts_with('s'), "{id}");
        assert_eq!(env.get(fleet_core::messages::SESSION_VAR), Some(&id));
    }

    #[test]
    fn a_spawn_runs_in_the_roles_directory_and_records_its_fleet_and_role() {
        let (root, state, wing) = crew("spawn-role", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let from = code(&root, "app").join("src");
        let _ = std::fs::create_dir_all(&from);
        let reply = spawn_in(&state, &tools, &root, &from, "worker");
        let id = spawned_id(&reply);
        assert_eq!(
            reply,
            Reply::Response(Response::Spawn(response::Spawn::Spawned {
                session: id.clone(),
                fleet: wing.clone(),
                role: "worker".into(),
                directory: wing.clone(),
            }))
        );
        let rows = crate::lock(&state).sessions().unwrap_or_default();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (
                rows[0].fleet.clone(),
                rows[0].seat.clone(),
                rows[0].role.clone()
            ),
            (Some(wing.clone()), None, Some("worker".into()))
        );
        let steps = crate::lock(&state).steps().unwrap_or_default();
        assert_eq!(steps.last().map(String::as_str), Some("SESSION_LAUNCHED"));
        let details = crate::lock(&state).step_details().unwrap_or_default();
        let launched = details.last().cloned().unwrap_or_default();
        assert!(
            launched.contains(r#""role":"worker""#) && !launched.contains(r#""seat""#),
            "{launched}"
        );
        assert!(
            launched.contains(r#""prompt":"You take tasks.""#),
            "{launched}"
        );
    }

    #[test]
    fn a_spawn_may_name_one_of_the_fleets_projects() {
        let (root, state, wing) = crew("spawn-named", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let lib = code(&root, "lib");
        let reply = spawn(&state, &tools, &store(&root), &wing, "worker", Some(&lib)).ok();
        assert!(
            matches!(&reply, Some(Reply::Response(Response::Spawn(response::Spawn::Spawned { directory, .. }))) if *directory == lib),
            "{reply:?}"
        );
    }

    #[test]
    fn a_named_project_is_matched_after_canonicalizing() {
        let (root, state, wing) = crew("spawn-canon", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let long_way = code(&root, "app").join("../lib");
        let lib = code(&root, "lib");
        let reply = spawn(
            &state,
            &tools,
            &store(&root),
            &wing,
            "worker",
            Some(&long_way),
        )
        .ok();
        assert!(
            matches!(&reply, Some(Reply::Response(Response::Spawn(response::Spawn::Spawned { directory, .. }))) if *directory == lib),
            "{reply:?}"
        );
    }

    #[test]
    fn a_directory_that_is_no_project_root_is_refused() {
        let (root, state, wing) = crew("spawn-not-project", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let src = code(&root, "app").join("src");
        let _ = std::fs::create_dir_all(&src);
        for named in [src, wing.clone()] {
            assert_eq!(
                spawn(&state, &tools, &store(&root), &wing, "worker", Some(&named)).ok(),
                Some(Reply::Status(Status::Session(
                    status::Session::NotAProject {
                        path: named.clone(),
                        fleet: wing.clone(),
                    }
                ))),
                "{}",
                named.display()
            );
        }
        assert_eq!(crate::lock(&state).sessions().ok(), Some(vec![]));
    }

    #[test]
    fn a_spawn_outside_every_fleet_or_of_no_such_role_is_refused() {
        let (root, state, wing) = crew("spawn-nowhere", &[("worker", "/a/claude")]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        assert_eq!(
            spawn_in(&state, &tools, &root, &root, "worker"),
            Reply::Status(Status::Session(status::Session::NoFleetHere {
                directory: root.clone()
            }))
        );
        assert_eq!(
            spawn_in(&state, &tools, &root, &wing, "critic"),
            Reply::Status(Status::Session(status::Session::NoSuchRole {
                role: "critic".into(),
                fleet: wing.clone(),
            }))
        );
    }

    /// Agent adapters that remove the crew's fleet while they answer a
    /// launch, as a slow fetch would let happen.
    struct RemovesFleet {
        state: Arc<Mutex<State>>,
        fleet: PathBuf,
    }

    impl AgentAdapters for RemovesFleet {
        fn launch(&self, a: &Path, r: &agent::Launch) -> Result<agent::Launched, AdapterFault> {
            let removed = crate::lock(&self.state).write(|w| w.remove_fleet(&self.fleet));
            assert!(removed.is_ok(), "{removed:?}");
            FakeAgents::default().launch(a, r)
        }
        fn read(&self, a: &Path, r: &agent::Read) -> Result<agent::Readings, AdapterFault> {
            FakeAgents::default().read(a, r)
        }
        fn protocol(&self, a: &Path) -> Result<agent::Speaks, AdapterFault> {
            FakeAgents::default().protocol(a)
        }
    }

    #[test]
    fn a_fleet_removed_while_the_adapter_answers_spawns_nothing() {
        let (root, state, wing) = crew("spawn-race", &[("worker", "/a/claude")]);
        let state = Arc::new(state);
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(RemovesFleet {
                state: state.clone(),
                fleet: wing.clone(),
            }),
            net: Box::new(FakeNet::default()),
        };
        assert_eq!(
            spawn_in(&state, &tools, &root, &wing, "worker"),
            Reply::Status(Status::Session(status::Session::NoFleetHere {
                directory: wing.clone()
            }))
        );
        assert_eq!(tools.mux.panes().ok(), Some(vec![]));
        assert_eq!(crate::lock(&state).sessions().ok(), Some(vec![]));
        let steps = crate::lock(&state).steps().unwrap_or_default();
        assert!(!steps.iter().any(|s| s == "SESSION_LAUNCHED"), "{steps:?}");
    }

    #[test]
    fn a_fleet_with_every_project_off_still_spawns() {
        let (root, state, wing) = crew("spawn-off", &[("worker", "/a/claude")]);
        assert!(
            crate::lock(&state)
                .write(|w| w.set_all_active(false))
                .is_ok()
        );
        let tools = tools(FakeMux::default(), FakeAgents::default());
        assert_ne!(
            spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker")),
            ""
        );
    }

    #[test]
    fn tending_never_revives_or_adopts_a_spawned_session() {
        let (root, state, wing) = crew("spawn-gone", &[("worker", "/a/claude")]);
        let mux = Arc::new(FakeMux::default());
        let tools = watched(&mux);
        let id = spawned_id(&spawn_in(&state, &tools, &root, &wing, "worker"));
        kill_all(&mux);
        // A tick, then a restarted supervisor's first tick.
        crate::tending::tick(
            &state,
            &tools,
            &store(&root),
            &crate::tending::Tending::new(),
            1_000,
        );
        crate::tending::tick(
            &state,
            &tools,
            &store(&root),
            &crate::tending::Tending::new(),
            2_000,
        );
        assert_eq!(
            mux.launched
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .len(),
            1,
            "launched once, by the spawn"
        );
        let (sessions, _) = spawned_of(&state, &tools, &wing);
        assert_eq!(
            sessions
                .iter()
                .map(|s| (s.session.clone(), s.state))
                .collect::<Vec<_>>(),
            [(id, SessionState::Gone)]
        );
    }

    /// A launch of `with_seat`'s seat as the tick read it: role `w` as its
    /// roster had it.
    fn seat_ask(state: &Mutex<State>, fleet: &Path) -> (Ask, ForSeat) {
        let roster = crate::lock(state).roster(fleet).unwrap_or_default();
        let role = roster.role("w").cloned();
        let digest = role
            .as_ref()
            .map(fleet_core::seats::Role::digest)
            .unwrap_or_default();
        let ask = Ask {
            adapter: "/a/claude".into(),
            directory: fleet.to_path_buf(),
            model: None,
            prompt: role.and_then(|r| r.prompt),
        };
        let seat = ForSeat {
            key: SeatKey {
                fleet: fleet.to_path_buf(),
                seat: "scribe".into(),
            },
            role: "w".into(),
            role_digest: digest,
            clear: vec![],
        };
        (ask, seat)
    }

    /// Whether launching `ask` started anything: a pane, a row, a step.
    fn started_anything(
        state: &Mutex<State>,
        tools: &Tools,
        root: &Path,
        (ask, seat): &(Ask, ForSeat),
    ) -> bool {
        let before = crate::lock(state).steps().unwrap_or_default().len();
        let _ = start_seat(state, tools, &store(root), ask, seat);
        let steps = crate::lock(state).steps().unwrap_or_default().len();
        !tools.mux.panes().unwrap_or_default().is_empty()
            || !crate::lock(state).sessions().unwrap_or_default().is_empty()
            || steps != before
    }

    /// A change to state made while a seat's launch is under way.
    type Change = fn(&mut state::Write<'_>, &Path) -> Result<(), StateError>;

    #[test]
    fn a_seat_no_longer_wanted_by_the_time_it_launches_starts_nothing() {
        // Each change lands while the adapter is fetched or asked, after the
        // tick read what was wanted.
        let changes: [(&str, Change); 4] = [
            ("off", |w, _| w.set_all_active(false)),
            ("seat removed", |w, f| {
                w.set_roster(f, &fleet_core::seats::Roster::default())
            }),
            ("role edited", |w, f| {
                let mut roster = with_seat_roster(f);
                roster.roles[0].prompt = Some("Another prompt.".into());
                w.set_roster(f, &roster)
            }),
            ("fleet removed", |w, f| w.remove_fleet(f)),
        ];
        for (what, change) in changes {
            let (root, state, fleet) =
                crate::testing::with_seat(&format!("sess-unwanted-{}", what.len()));
            let ask = seat_ask(&state, &fleet.directory);
            assert!(
                crate::lock(&state)
                    .write(|w| change(w, &fleet.directory))
                    .is_ok()
            );
            let t = tools(FakeMux::default(), FakeAgents::default());
            assert!(!started_anything(&state, &t, &root, &ask), "{what}");
        }
        // Still wanted, it launches.
        let (root, state, fleet) = crate::testing::with_seat("sess-wanted");
        let ask = seat_ask(&state, &fleet.directory);
        let t = tools(FakeMux::default(), FakeAgents::default());
        assert!(started_anything(&state, &t, &root, &ask));
    }

    /// `with_seat`'s roster.
    fn with_seat_roster(fleet: &Path) -> fleet_core::seats::Roster {
        use fleet_core::seats::{Role, Roster, Seat};
        Roster {
            roles: vec![Role {
                label: "w".into(),
                adapter: "/a/claude".into(),
                model: None,
                prompt: Some("You write the notes.".into()),
                directory: fleet.to_path_buf(),
            }],
            seats: vec![Seat {
                name: "scribe".into(),
                role: "w".into(),
            }],
        }
    }
}
