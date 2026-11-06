//! The supervisor's scenarios (testing.md, the in-process supervisor):
//! requests asked of one supervisor in process, its tmux, agents and
//! network faked, and every tick driven by hand at a time of the test's,
//! so the steps the modules record are the ones wait judges. A scenario
//! reaches its situation only as a person could: requests, `fleet.toml`
//! files and directories on disk, the fakes, the clock and a restart;
//! never a write to state. The rig they share is here; `seats` holds what
//! is launched, spawned and ended, `fleets` what create, remove, start and
//! stop change, `status` what status shows, `adapters` how a named adapter
//! is fetched and kept, `delivery` what is typed in and when, and `waits`
//! what wait and events answer.

mod adapters;
mod delivery;
mod fleets;
mod seats;
mod status;
mod waits;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use fleet_core::agent::{SessionState, StateName};
use fleet_core::messages::Sender;
use fleet_core::protocol::request::ConfigChoice;
use fleet_core::protocol::{Reply, Request, Response, Status, Waited, request, response};
use fleet_core::record::Step;
use state::{SessionRow, State};

use crate::Supervisor;
use crate::testing::{FakeAgents, FakeMux, FakeNet, code, store, tools, tree};

/// The fleet's config: one role, `w`, and one seat of it, `scribe`.
const CONFIG: &str = "[roles.w]\nadapter = \"/a/claude\"\nprompt = \"You write the notes.\"\n\n[seats.scribe]\nrole = \"w\"\n";

/// The fakes a rig drives, set up as a scenario needs them.
#[derive(Default)]
struct Fakes {
    mux: FakeMux,
    agents: FakeAgents,
    net: FakeNet,
}

/// A supervisor in process on a `tree`, its tmux, agents and network kept
/// to see and change.
struct Rig {
    sup: Supervisor,
    root: PathBuf,
    app: PathBuf,
    mux: Arc<FakeMux>,
    agents: Arc<FakeAgents>,
    net: Arc<FakeNet>,
}

/// `CONFIG` with no seats.
fn roles_only() -> &'static str {
    CONFIG.split("\n\n[seats").next().unwrap_or_default()
}

/// A rig whose `code/app` was made a default fleet by a create request
/// from `CONFIG`.
fn rig(name: &str) -> Rig {
    rig_with(name, Fakes::default())
}

/// [`rig`], on `fakes`.
fn rig_with(name: &str, fakes: Fakes) -> Rig {
    let rig = bare(name, fakes);
    assert!(std::fs::write(rig.app.join("fleet.toml"), CONFIG).is_ok());
    let created = rig.create(&rig.app, &[]);
    assert!(
        matches!(created, Reply::Response(Response::Create(_))),
        "{created:?}"
    );
    rig
}

/// A rig on `fakes` with no fleet yet.
fn bare(name: &str, fakes: Fakes) -> Rig {
    let root = tree(name);
    let (mux, agents, net) = (
        Arc::new(fakes.mux),
        Arc::new(fakes.agents),
        Arc::new(fakes.net),
    );
    Rig {
        sup: supervisor(&root, &mux, &agents, &net),
        app: code(&root, "app"),
        root,
        mux,
        agents,
        net,
    }
}

/// A rig on `fakes` whose `wing/` was made a wingman fleet serving
/// `code/app` and `code/lib`, by a create request adopting a config with
/// `roles` and no seats: the rig, and `wing/`.
fn crew(name: &str, fakes: Fakes, roles: &str) -> (Rig, PathBuf) {
    let rig = bare(name, fakes);
    let wing = rig.root.join("wing");
    let config = format!("projects = [\"../code/app\", \"../code/lib\"]\n\n{roles}");
    assert!(std::fs::write(wing.join("fleet.toml"), config).is_ok());
    let created = rig.create(&wing, &[]);
    assert!(
        matches!(created, Reply::Response(Response::Create(_))),
        "{created:?}"
    );
    (rig, wing)
}

/// A supervisor started on the state under `root`, driving the fakes.
fn supervisor(
    root: &Path,
    mux: &Arc<FakeMux>,
    agents: &Arc<FakeAgents>,
    net: &Arc<FakeNet>,
) -> Supervisor {
    let state = State::open(&root.join("st"));
    assert!(state.is_ok());
    let tools = tools(mux.clone(), agents.clone(), net.clone());
    Supervisor::new(state.unwrap_or_else(|_| unreachable!()), tools, store(root))
}

impl Rig {
    fn ask(&self, request: Request) -> Reply {
        self.sup.ask(request).unwrap_or_else(|e| {
            Reply::Status(Status::Malformed {
                error: format!("{e:?}"),
            })
        })
    }

    fn tick(&self, now_millis: i64) {
        self.sup.tick(now_millis);
    }

    /// The same state, tools and adapters, with a tending loop's memory
    /// fresh: the supervisor restarted.
    fn restarted(&self) -> Supervisor {
        supervisor(&self.root, &self.mux, &self.agents, &self.net)
    }

    fn create(&self, directory: &Path, projects: &[PathBuf]) -> Reply {
        self.ask(Request::Create(request::Create::Fleet {
            directory: directory.to_path_buf(),
            projects: projects.to_vec(),
        }))
    }

    fn start_all(&self) -> Reply {
        self.ask(Request::Start(request::Start::All))
    }

    fn stop_all(&self) -> Reply {
        self.ask(Request::Stop(request::Stop::All))
    }

    fn remove(&self, directory: &Path, projects: &[PathBuf], config: ConfigChoice) -> Reply {
        self.ask(Request::Remove(request::Remove::Fleet {
            directory: directory.to_path_buf(),
            projects: projects.to_vec(),
            config,
        }))
    }

    /// The kind of every step recorded, oldest first.
    fn kinds(&self) -> Vec<String> {
        self.record()
            .iter()
            .map(|s| s.to_row().map(|(kind, _)| kind).unwrap_or_default())
            .collect()
    }

    /// Every step recorded, oldest first, as events from outside every
    /// fleet tell them.
    fn record(&self) -> Vec<Step> {
        let mut steps = Vec::new();
        let mut from = 0;
        loop {
            let reply = self.ask(Request::Events(request::Events::Follow {
                directory: self.root.clone(),
                from: Some(from),
                hold_millis: 0,
            }));
            let Reply::Response(Response::Events(response::Events::Steps { events, next, .. })) =
                reply
            else {
                assert!(matches!(reply, Reply::Response(_)), "{reply:?}");
                return steps;
            };
            if events.is_empty() {
                return steps;
            }
            steps.extend(events.into_iter().map(|e| e.step));
            from = next;
        }
    }

    /// What status shows from `from`; an empty overview, after failing the
    /// test, for any other reply.
    fn status(&self, from: &Path) -> response::Overview {
        match self.ask(Request::Overview {
            directory: from.to_path_buf(),
        }) {
            Reply::Response(Response::Overview(o)) => o,
            other => {
                assert!(matches!(other, Reply::Response(_)), "{other:?}");
                response::Overview::default()
            }
        }
    }

    /// The id of the message queued for `to`, sent from the fleet; empty
    /// when it was refused.
    fn send(&self, to: &str, text: &str) -> String {
        match self.send_reply(to, text) {
            Reply::Response(Response::Message(response::Message::Queued { message, .. })) => {
                message
            }
            _ => String::new(),
        }
    }

    fn send_reply(&self, to: &str, text: &str) -> Reply {
        self.ask(Request::Message(request::Message::Send {
            directory: self.app.clone(),
            to: to.into(),
            text: text.into(),
            sender: Sender::Script,
        }))
    }

    /// The id of a session spawned from role `w` of the fleet; empty when
    /// it was refused.
    fn spawn(&self) -> String {
        spawned(&self.spawn_from(&self.app, "w", None))
    }

    /// A spawn from role `role` of the fleet holding `from`, working in
    /// `working_directory` when given.
    fn spawn_from(&self, from: &Path, role: &str, working_directory: Option<PathBuf>) -> Reply {
        self.ask(Request::Spawn(request::Spawn::Session {
            directory: from.to_path_buf(),
            role: role.into(),
            working_directory,
        }))
    }

    fn end(&self, from: &Path, name: &str) -> Reply {
        self.ask(Request::End(request::End::Session {
            directory: from.to_path_buf(),
            name: name.into(),
        }))
    }

    /// A first poll of a wait on `name` from the fleet, answered from what
    /// is recorded.
    fn wait(&self, name: &str, states: &[StateName]) -> Reply {
        self.ask(Request::Wait(request::Wait::For {
            directory: self.app.clone(),
            name: name.into(),
            states: states.to_vec(),
            hold_millis: 0,
        }))
    }

    /// A later poll of a wait on `on`, from record id `from`.
    fn wait_on(&self, on: Waited, states: &[StateName], from: i64) -> Reply {
        self.ask(Request::Wait(request::Wait::On {
            on,
            states: states.to_vec(),
            from,
            hold_millis: 0,
        }))
    }

    fn scribe(&self) -> Waited {
        Waited::Seat {
            fleet: self.app.clone(),
            seat: "scribe".into(),
        }
    }

    /// The seat's sessions, oldest first.
    fn seat_rows(&self) -> Vec<SessionRow> {
        self.sup
            .lock()
            .sessions()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.seat.is_some())
            .collect()
    }

    /// The seat's session: its first row.
    fn seated(&self) -> SessionRow {
        let rows = self.seat_rows();
        assert_eq!(rows.len(), 1, "{rows:?}");
        rows.into_iter().next().unwrap_or_else(|| unreachable!())
    }
}

/// A rig's tree goes with it.
impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The id a spawn answered with; empty for any other reply.
fn spawned(reply: &Reply) -> String {
    match reply {
        Reply::Response(Response::Spawn(response::Spawn::Spawned { session, .. })) => {
            session.clone()
        }
        _ => String::new(),
    }
}

/// The record id a waiting reply says to ask from next; -1 for any other.
fn next_of(reply: &Reply) -> i64 {
    match reply {
        Reply::Response(Response::Wait(response::Wait::Waiting { next, .. })) => *next,
        _ => -1,
    }
}

fn waiting_on(reply: &Reply) -> Option<&Waited> {
    match reply {
        Reply::Response(Response::Wait(response::Wait::Waiting { on, .. })) => Some(on),
        _ => None,
    }
}

fn reached(on: Waited, session: &str, state: SessionState) -> Reply {
    Reply::Response(Response::Wait(response::Wait::Reached {
        on,
        session: session.into(),
        state,
    }))
}

fn refused(why: fleet_core::status::Wait) -> Reply {
    Reply::Status(why.into())
}
