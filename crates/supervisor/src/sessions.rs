//! Agents' sessions: launched through an adapter into tmux, listed with
//! each one's state, and stopped.

use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::path::PathBuf;
use std::sync::Mutex;

use fleet_core::agent::{self, Verb};
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::seats::SeatKey;
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
    /// The digest of the role it is launched from.
    pub(crate) role_digest: String,
    /// Its sessions whose pane ended, forgotten and ended first.
    pub(crate) clear: Vec<String>,
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

/// Launches `fleet debug`'s agent: the adapter says what to run, tmux runs
/// it, and the session and its step are recorded together, or not at all.
pub(crate) fn launch(
    state: &Mutex<State>,
    tools: &Tools,
    store: &adapters::Store,
    ask: &Ask,
) -> Result<Reply, Error> {
    let p = match prepare(tools, store, ask, None) {
        Ok(p) => p,
        Err(status) => return Ok(Reply::Status(status)),
    };
    let written = lock(state).write(|w| launch_in(w, tools, &p));
    Ok(match settle(written, tools, &p.row.id)? {
        Ok(()) => Reply::Response(Response::Session(response::Session::Launched {
            session: p.row.id,
        })),
        Err(status) => Reply::Status(status),
    })
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
    let p = match prepare(tools, store, ask, Some(seat)) {
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
            if w.stop_session(gone, || tools.mux.stop(gone))? {
                w.record(&Step::SessionGone {
                    session: gone.clone(),
                    fleet: seat.key.fleet.display().to_string(),
                    seat: seat.key.seat.clone(),
                })?;
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

/// Checks `ask`'s directory, resolves its adapter (fetching it on first
/// use) and asks it what to run, for `seat` if any. No state is touched.
fn prepare(
    tools: &Tools,
    store: &adapters::Store,
    ask: &Ask,
    seat: Option<&ForSeat>,
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
    let launched = tools.agents.launch(&program, &request).map_err(|fault| {
        Status::Session(status::Session::AdapterFailed {
            adapter: program.clone(),
            verb: Verb::Launch,
            fault,
        })
    })?;
    let row = SessionRow {
        id: new_id(),
        adapter: program,
        directory: ask.directory.clone(),
        launch_time_millis: now_millis(),
        pane: String::new(),
        fleet: seat.map(|s| s.key.fleet.clone()),
        seat: seat.map(|s| s.key.seat.clone()),
        role_digest: seat.map(|s| s.role_digest.clone()),
    };
    let step = Step::SessionLaunched {
        session: row.id.clone(),
        adapter: ask.adapter.clone(),
        program: row.adapter.display().to_string(),
        directory: ask.directory.display().to_string(),
        model: ask.model.clone(),
        prompt: ask.prompt.clone(),
        fleet: seat.map(|s| s.key.fleet.display().to_string()),
        seat: seat.map(|s| s.key.seat.clone()),
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

/// Stops a session: ends its tmux session and forgets it, recording the
/// step, or changes nothing.
pub(crate) fn stop(state: &Mutex<State>, tools: &Tools, session: &str) -> Result<Reply, Error> {
    let step = Step::SessionStopped {
        session: session.to_owned(),
    };
    let written = lock(state).write(|w| {
        let had = w.stop_session(session, || tools.mux.stop(session))?;
        if had {
            w.record(&step)?;
        }
        Ok(had)
    });
    match written {
        Ok(true) => Ok(Reply::Response(Response::Session(
            response::Session::Stopped {
                session: session.to_owned(),
            },
        ))),
        Ok(false) => Ok(Reply::Status(Status::Session(
            status::Session::NoSuchSession {
                session: session.to_owned(),
            },
        ))),
        Err(StateError::Tool(e)) => Ok(Reply::Status(tool_status(e))),
        Err(e) => Err(e.into()),
    }
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

/// A new session id: `s` and ten lowercase hex digits, safe as a tmux
/// session's name.
fn new_id() -> String {
    let mut bytes = [0u8; 5];
    let random = File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if random.is_err() {
        bytes.copy_from_slice(&now_millis().to_le_bytes()[..5]);
    }
    bytes.iter().fold(String::from("s"), |mut id, b| {
        let _ = write!(id, "{b:02x}");
        id
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};
    use std::sync::{Mutex, PoisonError};

    use fleet_core::agent::{AdapterFault, SessionState, Verb};
    use fleet_core::protocol::{Reply, Response, Status, response};
    use fleet_core::tools::Pane;

    use crate::Tools;
    use crate::testing::{FakeAgents, FakeMux, Shared, tree};

    fn tools(mux: FakeMux, adapters: FakeAgents) -> Tools {
        Tools {
            mux: Box::new(mux),
            agents: Box::new(adapters),
            net: Box::new(crate::testing::FakeNet::default()),
        }
    }

    /// The id a launch answered with; empty for any other reply.
    fn launched(reply: &Reply) -> String {
        match reply {
            Reply::Response(Response::Session(response::Session::Launched { session })) => {
                session.clone()
            }
            _ => String::new(),
        }
    }

    /// What `fleet status` answers from outside every fleet, where these
    /// sessions of no seat are loose.
    fn list(state: &Mutex<State>, tools: &Tools) -> Result<Reply, crate::Error> {
        crate::overview::overview(
            state,
            tools,
            &crate::tending::Tending::new(),
            Path::new("/"),
        )
    }

    /// The loose sessions and faults an overview answered with; none for
    /// any other reply.
    fn listed(reply: &Reply) -> (Vec<response::Listed>, Vec<response::Fault>) {
        match reply {
            Reply::Response(Response::Overview(o)) => (o.loose.clone(), o.faults.clone()),
            _ => (vec![], vec![]),
        }
    }

    /// The tree's adapter store, fetching from a base no test serves.
    fn store(root: &Path) -> crate::adapters::Store {
        crate::adapters::Store::new(&root.join("st"), Some("https://r".into()))
    }

    fn ask(adapter: &str, directory: &Path) -> Ask {
        Ask {
            adapter: adapter.to_owned(),
            directory: directory.to_path_buf(),
            model: Some("sonnet".into()),
            prompt: None,
        }
    }

    #[test]
    fn a_launched_session_lists_with_the_adapters_reading() {
        let (root, state) = tree("sess-list", &["app"]);
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let app = root.join("code/app");
        let reply = launch(&state, &tools, &store(&root), &ask("/a/claude", &app));
        let id = launched(&reply.unwrap_or(Reply::Status(Status::Project(
            status::Project::NotServed { path: app.clone() },
        ))));
        assert!(id.starts_with('s') && id.len() == 11, "{id}");
        let (sessions, faults) = listed(&list(&state, &tools).unwrap_or(Reply::Status(
            Status::Project(status::Project::NotServed { path: app.clone() }),
        )));
        assert_eq!(faults, []);
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session, id);
        assert_eq!(sessions[0].adapter, PathBuf::from("/a/claude"));
        assert_eq!(sessions[0].directory, app);
        assert_eq!(sessions[0].state, SessionState::Idle);
    }

    #[test]
    fn a_session_whose_pane_ended_is_gone_without_asking_the_adapter() {
        let (root, state) = tree("sess-gone", &["app"]);
        let mux = FakeMux::default();
        let adapters = FakeAgents::default();
        let tools = tools(mux, adapters);
        let app = root.join("code/app");
        assert!(launch(&state, &tools, &store(&root), &ask("/a/claude", &app)).is_ok());
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
            net: Box::new(crate::testing::FakeNet::default()),
        };
        let (sessions, faults) = listed(&list(&state, &tools).unwrap_or(Reply::Status(
            Status::Project(status::Project::NotServed { path: app.clone() }),
        )));
        assert_eq!(sessions[0].state, SessionState::Gone);
        assert_eq!(faults, []);
    }

    #[test]
    fn an_adapter_fault_makes_its_sessions_unknown_and_says_why() {
        let (root, state) = tree("sess-fault", &["app"]);
        let app = root.join("code/app");
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
        assert!(launch(&state, &tools, &store(&root), &ask("/a/claude", &app)).is_ok());
        let (sessions, faults) = listed(&list(&state, &tools).unwrap_or(Reply::Status(
            Status::Project(status::Project::NotServed { path: app.clone() }),
        )));
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
        let (root, state) = tree("sess-tmux-fails", &["app"]);
        let app = root.join("code/app");
        let tools = tools(
            FakeMux {
                fail_launch: true,
                ..FakeMux::default()
            },
            FakeAgents::default(),
        );
        let reply = launch(&state, &tools, &store(&root), &ask("/a/claude", &app)).ok();
        assert!(
            matches!(
                reply,
                Some(Reply::Status(Status::Session(
                    status::Session::MultiplexerFailed { .. }
                )))
            ),
            "{reply:?}"
        );
        assert_eq!(crate::lock(&state).sessions().ok(), Some(vec![]));
    }

    #[test]
    fn a_tmux_that_is_not_installed_is_said_so() {
        let (root, state) = tree("sess-no-tmux", &["app"]);
        let app = root.join("code/app");
        let tools = tools(
            FakeMux {
                missing: true,
                ..FakeMux::default()
            },
            FakeAgents::default(),
        );
        let refused = Some(Reply::Status(Status::Session(
            status::Session::MultiplexerCouldNotRun {
                error: std::io::Error::from(std::io::ErrorKind::NotFound).to_string(),
            },
        )));
        assert_eq!(
            launch(&state, &tools, &store(&root), &ask("/a/claude", &app)).ok(),
            refused
        );
        assert_eq!(crate::lock(&state).sessions().ok(), Some(vec![]));
    }

    #[test]
    fn a_missing_directory_is_refused_before_the_adapter_runs() {
        let (root, state) = tree("sess-missing", &["app"]);
        let adapters = FakeAgents::default();
        let tools = tools(FakeMux::default(), adapters);
        let nope = root.join("nope");
        let reply = launch(&state, &tools, &store(&root), &ask("/a/claude", &nope)).ok();
        assert_eq!(
            reply,
            Some(Reply::Status(Status::File(status::File::Missing {
                path: nope
            })))
        );
    }

    #[test]
    fn an_adapter_that_cannot_launch_is_refused_with_its_fault() {
        let (root, state) = tree("sess-launch-fault", &["app"]);
        let app = root.join("code/app");
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
        let reply = launch(&state, &tools, &store(&root), &ask("/a/claude", &app)).ok();
        assert_eq!(
            reply,
            Some(Reply::Status(Status::Session(
                status::Session::AdapterFailed {
                    adapter: PathBuf::from("/a/claude"),
                    verb: Verb::Launch,
                    fault
                }
            )))
        );
    }

    #[test]
    fn stopping_twice_says_there_is_no_such_session() {
        let (root, state) = tree("sess-stop", &["app"]);
        let app = root.join("code/app");
        let tools = tools(FakeMux::default(), FakeAgents::default());
        let reply = launch(&state, &tools, &store(&root), &ask("/a/claude", &app));
        let id = launched(&reply.unwrap_or(Reply::Status(Status::Project(
            status::Project::NotServed { path: app.clone() },
        ))));
        assert_eq!(
            stop(&state, &tools, &id).ok(),
            Some(Reply::Response(Response::Session(
                response::Session::Stopped {
                    session: id.clone()
                }
            )))
        );
        assert_eq!(tools.mux.panes().ok(), Some(vec![]));
        assert_eq!(
            stop(&state, &tools, &id).ok(),
            Some(Reply::Status(Status::Session(
                status::Session::NoSuchSession { session: id }
            )))
        );
    }

    #[test]
    fn a_named_adapter_is_fetched_then_launched() {
        let (root, state) = tree("sess-named", &["app"]);
        let app = root.join("code/app");
        let adapters = std::sync::Arc::new(FakeAgents::default());
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(Shared(adapters.clone())),
            net: Box::new(crate::testing::FakeNet {
                files: crate::testing::good_release("https://r"),
                ..crate::testing::FakeNet::default()
            }),
        };
        let reply = launch(
            &state,
            &tools,
            &store(&root),
            &ask("claude-code@2.1.285", &app),
        );
        assert!(
            reply.as_ref().is_ok_and(|r| !launched(r).is_empty()),
            "{reply:?}"
        );
        let kept = root.join("st/adapters/claude-code/2.1.285/fleet-claude-code");
        let launches = adapters
            .launches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        assert_eq!(launches, vec![kept.clone()]);
        let (sessions, _) = listed(&list(&state, &tools).unwrap_or_else(|_| {
            Reply::Status(Status::Project(status::Project::NotServed {
                path: "/".into(),
            }))
        }));
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
        let (root, state) = tree("sess-bare", &["app"]);
        let app = root.join("code/app");
        let adapters = std::sync::Arc::new(FakeAgents::default());
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(Shared(adapters.clone())),
            net: Box::new(crate::testing::FakeNet::default()),
        };
        let reply = launch(&state, &tools, &store(&root), &ask("claude-code", &app)).ok();
        assert_eq!(
            reply,
            Some(Reply::Status(Status::Adapter(
                status::Adapter::AdapterUnpinned {
                    name: "claude-code".into(),
                    latest: None
                }
            )))
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
        let (root, state) = tree("sess-two", &["app"]);
        let app = root.join("code/app");
        let adapters = std::sync::Arc::new(FakeAgents::default());
        let tools = Tools {
            mux: Box::new(FakeMux::default()),
            agents: Box::new(Shared(adapters.clone())),
            net: Box::new(crate::testing::FakeNet::default()),
        };
        for adapter in ["/a/one", "/a/two", "/a/one"] {
            assert!(launch(&state, &tools, &store(&root), &ask(adapter, &app)).is_ok());
        }
        assert!(list(&state, &tools).is_ok());
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
