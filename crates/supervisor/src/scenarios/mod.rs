//! Delivery's scenarios across the supervisor's modules (testing.md, the
//! in-process supervisor): requests asked of one supervisor in process, its
//! tmux and agents faked, and every tick driven by hand at a time of the
//! test's, so the steps the modules record are the ones wait judges. The
//! rig they share is here; `delivery` holds what is typed in and when, and
//! `waits` what wait and events answer.

mod delivery;
mod waits;

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};

use fleet_core::agent::{SessionState, StateName};
use fleet_core::messages::Sender;
use fleet_core::protocol::{Reply, Request, Response, Status, Waited, request, response};
use fleet_core::status;
use state::{SessionRow, State};

use crate::Supervisor;
use crate::testing::{FakeAgents, FakeMux, code, store, tools, tree};

/// The fleet's config: one role, `w`, and one seat of it, `scribe`.
const CONFIG: &str = "[roles.w]\nadapter = \"/a/claude\"\nprompt = \"You write the notes.\"\n\n[seats.scribe]\nrole = \"w\"\n";

/// A supervisor in process, its tmux and agents kept to see and change, on
/// a tree whose `code/app` was made a default fleet by a create request
/// from `CONFIG`.
struct Rig {
    sup: Supervisor,
    root: PathBuf,
    app: PathBuf,
    mux: Arc<FakeMux>,
    agents: Arc<FakeAgents>,
}

fn rig(name: &str) -> Rig {
    rig_on(name, FakeMux::default())
}

fn rig_on(name: &str, mux: FakeMux) -> Rig {
    let (root, state) = tree(name, &["app", "api"]);
    let app = code(&root, "app");
    assert!(std::fs::write(app.join("fleet.toml"), CONFIG).is_ok());
    let (mux, agents) = (Arc::new(mux), Arc::new(FakeAgents::default()));
    let state = state.into_inner().unwrap_or_else(PoisonError::into_inner);
    let tools = tools(mux.clone(), agents.clone());
    let rig = Rig {
        sup: Supervisor::new(state, tools, store(&root)),
        root,
        app,
        mux,
        agents,
    };
    let created = rig.create(&rig.app);
    assert!(
        matches!(created, Reply::Response(Response::Create(_))),
        "{created:?}"
    );
    rig
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
        let state = State::open(&self.root.join("st"));
        assert!(state.is_ok());
        let tools = tools(self.mux.clone(), self.agents.clone());
        Supervisor::new(
            state.unwrap_or_else(|_| unreachable!()),
            tools,
            store(&self.root),
        )
    }

    fn create(&self, directory: &std::path::Path) -> Reply {
        self.ask(Request::Create(request::Create::Fleet {
            directory: directory.to_path_buf(),
            projects: vec![],
        }))
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

    fn spawn(&self) -> String {
        match self.ask(Request::Spawn(request::Spawn::Session {
            directory: self.app.clone(),
            role: "w".into(),
            working_directory: None,
        })) {
            Reply::Response(Response::Spawn(response::Spawn::Spawned { session, .. })) => session,
            _ => String::new(),
        }
    }

    fn end(&self, name: &str) -> Reply {
        self.ask(Request::End(request::End::Session {
            directory: self.app.clone(),
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
        crate::lock(&self.sup.state)
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

fn refused(why: status::Wait) -> Reply {
    Reply::Status(why.into())
}
