//! Agents' sessions: launched through an adapter into tmux, listed with
//! each one's state, and stopped.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::agent::{self, Reading, SessionRef, Verb};
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::status;
use fleet_core::tools::{Pane, ToolError};
use state::{SessionRow, State, StateError, Step, now_millis};

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

/// Launches an agent: the adapter says what to run, tmux runs it, and the
/// session and its step are recorded together, or not at all.
pub(crate) fn launch(
    state: &Mutex<State>,
    tools: &Tools,
    store: &adapters::Store,
    ask: &Ask,
) -> Result<Reply, Error> {
    if !ask.directory.is_dir() {
        return Ok(Reply::Status(Status::File(status::File::Missing {
            path: ask.directory.clone(),
        })));
    }
    let program = match adapters::resolve(store, tools, &ask.adapter) {
        Ok(program) => program,
        Err(status) => return Ok(Reply::Status(status)),
    };
    let request = agent::Launch {
        directory: ask.directory.clone(),
        model: ask.model.clone(),
        prompt: ask.prompt.clone(),
    };
    let launched = match tools.agents.launch(&program, &request) {
        Ok(launched) => launched,
        Err(fault) => {
            return Ok(Reply::Status(Status::Session(
                status::Session::AdapterFailed {
                    adapter: program.clone(),
                    verb: Verb::Launch,
                    fault,
                },
            )));
        }
    };
    let row = SessionRow {
        id: new_id(),
        adapter: program,
        directory: ask.directory.clone(),
        launch_time_millis: now_millis(),
    };
    let step = Step::SessionLaunched {
        session: row.id.clone(),
        adapter: ask.adapter.clone(),
        directory: ask.directory.display().to_string(),
        model: ask.model.clone(),
        prompt: ask.prompt.clone(),
    };
    let written = lock(state).write(|w| {
        w.launch_session(&row, || {
            tools.mux.launch(&row.id, &row.directory, &launched)
        })?;
        w.record(&step)
    });
    match written {
        Ok(()) => Ok(Reply::Response(Response::Session(
            response::Session::Launched { session: row.id },
        ))),
        Err(StateError::Tool(e)) => Ok(Reply::Status(tool_status(e))),
        Err(e) => {
            // The session may be running with no record of it: end it.
            let _ = tools.mux.stop(&row.id);
            Err(e.into())
        }
    }
}

/// Every session with its state: tmux says which panes are alive, and each
/// adapter, asked once for all of its live sessions, what its agents are
/// doing.
pub(crate) fn list(state: &Mutex<State>, tools: &Tools) -> Result<Reply, Error> {
    let rows = lock(state).sessions()?;
    let panes = match tools.mux.panes() {
        Ok(panes) => panes,
        Err(e) => return Ok(Reply::Status(tool_status(e))),
    };
    let panes: BTreeMap<&str, &Pane> = panes.iter().map(|p| (p.session.as_str(), p)).collect();
    let mut asks: BTreeMap<&Path, Vec<SessionRef>> = BTreeMap::new();
    for row in &rows {
        if let Some(pane) = panes.get(row.id.as_str()).filter(|p| !p.dead) {
            asks.entry(&row.adapter).or_default().push(SessionRef {
                session: row.id.clone(),
                pid: pane.pid,
                directory: row.directory.clone(),
            });
        }
    }
    let mut readings: BTreeMap<String, Reading> = BTreeMap::new();
    let mut faults = Vec::new();
    for (adapter, sessions) in asks {
        let asked: Vec<String> = sessions.iter().map(|s| s.session.clone()).collect();
        match tools.agents.read(adapter, &agent::Read { sessions }) {
            Ok(answer) => readings.extend(
                answer
                    .sessions
                    .into_iter()
                    .filter(|r| asked.contains(&r.session))
                    .map(|r| (r.session.clone(), r)),
            ),
            Err(fault) => faults.push(response::Fault {
                adapter: adapter.to_path_buf(),
                fault,
            }),
        }
    }
    let now = now_millis();
    let sessions = rows
        .into_iter()
        .map(|row| response::Listed {
            state: agent::state_of(
                panes.get(row.id.as_str()).copied(),
                readings.get(&row.id),
                row.launch_time_millis,
                now,
            ),
            session: row.id,
            adapter: row.adapter,
            directory: row.directory,
            launch_time_millis: row.launch_time_millis,
        })
        .collect();
    Ok(Reply::Response(Response::Session(
        response::Session::Listed { sessions, faults },
    )))
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
fn tool_status(error: ToolError) -> Status {
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
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::{Mutex, PoisonError};

    use fleet_core::agent::{
        self, Activity, AdapterFault, Launched, Reading, Readings, SessionState, Speaks, Verb,
    };
    use fleet_core::protocol::{Reply, Response, Status, response};
    use fleet_core::tools::{AgentAdapters, Multiplexer, Pane, ToolError};

    use crate::Tools;
    use crate::testing::tree;

    #[derive(Default)]
    struct FakeMux {
        panes: Mutex<Vec<Pane>>,
        fail_launch: bool,
        /// Like a tmux that is not installed.
        missing: bool,
    }

    fn not_installed() -> ToolError {
        ToolError::CouldNotRun {
            program: "tmux".into(),
            error: std::io::Error::from(std::io::ErrorKind::NotFound),
        }
    }

    impl Multiplexer for FakeMux {
        fn launch(&self, session: &str, _: &Path, _: &Launched) -> Result<(), ToolError> {
            if self.missing {
                return Err(not_installed());
            }
            if self.fail_launch {
                return Err(ToolError::Failed {
                    command: "tmux new-session".into(),
                    said: "no".into(),
                });
            }
            let mut panes = self.panes.lock().unwrap_or_else(PoisonError::into_inner);
            let pid = 100 + u32::try_from(panes.len()).unwrap_or(0);
            panes.push(Pane {
                session: session.into(),
                pid,
                dead: false,
            });
            Ok(())
        }

        fn panes(&self) -> Result<Vec<Pane>, ToolError> {
            if self.missing {
                return Err(not_installed());
            }
            Ok(self
                .panes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone())
        }

        fn stop(&self, session: &str) -> Result<(), ToolError> {
            self.panes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .retain(|p| p.session != session);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeAgents {
        launches: Mutex<Vec<PathBuf>>,
        reads: Mutex<Vec<(PathBuf, usize)>>,
        launch_fault: Option<AdapterFault>,
        read_fault: Option<AdapterFault>,
    }

    impl AgentAdapters for FakeAgents {
        fn launch(&self, adapter: &Path, _: &agent::Launch) -> Result<Launched, AdapterFault> {
            self.launches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(adapter.to_path_buf());
            match &self.launch_fault {
                Some(fault) => Err(fault.clone()),
                None => Ok(Launched {
                    argv: vec!["/bin/agent".into()],
                    env: BTreeMap::new(),
                }),
            }
        }

        fn read(&self, adapter: &Path, request: &agent::Read) -> Result<Readings, AdapterFault> {
            self.reads
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((adapter.to_path_buf(), request.sessions.len()));
            if let Some(fault) = &self.read_fault {
                return Err(fault.clone());
            }
            Ok(Readings {
                sessions: request
                    .sessions
                    .iter()
                    .map(|s| Reading {
                        session: s.session.clone(),
                        activity: Activity::Idle,
                        reason: None,
                    })
                    .collect(),
            })
        }

        fn protocol(&self, _: &Path) -> Result<Speaks, AdapterFault> {
            Ok(Speaks { protocol: 1 })
        }
    }

    /// Agent adapters a test keeps a handle on, to see what was asked of them.
    struct Shared(std::sync::Arc<FakeAgents>);
    impl AgentAdapters for Shared {
        fn launch(&self, a: &Path, r: &agent::Launch) -> Result<Launched, AdapterFault> {
            self.0.launch(a, r)
        }
        fn read(&self, a: &Path, r: &agent::Read) -> Result<Readings, AdapterFault> {
            self.0.read(a, r)
        }
        fn protocol(&self, a: &Path) -> Result<Speaks, AdapterFault> {
            self.0.protocol(a)
        }
    }

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

    /// The sessions and faults a list answered with; none for any other
    /// reply.
    fn listed(reply: &Reply) -> (Vec<response::Listed>, Vec<response::Fault>) {
        match reply {
            Reply::Response(Response::Session(response::Session::Listed { sessions, faults })) => {
                (sessions.clone(), faults.clone())
            }
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
        let mut reads = adapters
            .reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        reads.sort();
        assert_eq!(
            reads,
            [(PathBuf::from("/a/one"), 2), (PathBuf::from("/a/two"), 1)]
        );
    }
}
