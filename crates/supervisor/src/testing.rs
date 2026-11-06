//! What the supervisor's tests share.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use fleet_core::agent::{
    self, Activity, AdapterFault, Launched, Reading, Readings, SessionRef, Speaks,
};
use fleet_core::tools::{AgentAdapters, Multiplexer, Pane, ToolError};

use state::MessageRow;

use crate::adapters::Store;
use crate::{Supervisor, Tools};

/// A temp tree with repositories `app`, `api`, `blog` and `lib` under
/// `code/`, a `wing/` directory, and a state directory `st/`, all
/// canonical: its root.
pub(crate) fn tree(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("fleet-sup-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for repo in ["app", "api", "blog", "lib"] {
        let _ = fs::create_dir_all(root.join("code").join(repo).join(".git"));
    }
    let _ = fs::create_dir_all(root.join("wing"));
    let _ = fs::create_dir_all(root.join("st"));
    fs::canonicalize(&root).unwrap_or(root)
}

/// The repository `repo` in a `tree`.
pub(crate) fn code(root: &std::path::Path, repo: &str) -> PathBuf {
    root.join("code").join(repo)
}

/// A network serving files from memory: a URL it holds is saved, any other
/// is not found. It records every URL asked for.
#[derive(Default)]
pub(crate) struct FakeNet {
    /// URL → the file's bytes.
    pub(crate) files: std::collections::BTreeMap<String, Vec<u8>>,
    /// Every URL asked for, in order.
    pub(crate) asked: Mutex<Vec<String>>,
    /// Whether curl is missing.
    pub(crate) no_curl: bool,
    /// How long each download takes, to make two overlap.
    pub(crate) delay: std::time::Duration,
}

impl FakeNet {
    /// How many downloads were asked for.
    pub(crate) fn count(&self) -> usize {
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
    }
}

impl fleet_core::tools::Network for FakeNet {
    fn download(
        &self,
        url: &str,
        to: &std::path::Path,
    ) -> Result<fleet_core::tools::Fetched, fleet_core::tools::ToolError> {
        use fleet_core::tools::{Fetched, ToolError};
        self.asked
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(url.to_owned());
        std::thread::sleep(self.delay);
        if self.no_curl {
            return Err(ToolError::CouldNotRun {
                program: "curl".into(),
                error: std::io::Error::from(std::io::ErrorKind::NotFound),
            });
        }
        match self.files.get(url) {
            Some(bytes) => fs::write(to, bytes)
                .map(|()| Fetched::Saved)
                .map_err(|error| ToolError::File {
                    doing: fleet_core::tools::FileStep::Write,
                    path: to.to_path_buf(),
                    error,
                }),
            None => Ok(Fetched::NotFound),
        }
    }

    fn version(&self) -> Result<String, fleet_core::tools::ToolError> {
        Ok("curl 8.0.0".into())
    }
}

/// What a fake adapter's build holds.
pub(crate) const BINARY: &[u8] = b"#!/bin/sh\necho the adapter\n";

/// claude-code 2.1.285's release, as `base` serves it: its manifest says
/// `protocol`, and its build for this machine (`BINARY`) has `hash`.
pub(crate) fn release(
    base: &str,
    protocol: u32,
    hash: &str,
) -> std::collections::BTreeMap<String, Vec<u8>> {
    let t = fleet_core::adapters::target().unwrap_or("none");
    let manifest = format!(
        r#"{{"name":"claude-code","version":"2.1.285","protocol":{protocol},"targets":{{"{t}":"{hash}"}}}}"#
    );
    std::collections::BTreeMap::from([
        (
            format!("{base}/claude-code-v2.1.285/manifest.json"),
            manifest.into_bytes(),
        ),
        (
            format!("{base}/claude-code-v2.1.285/fleet-claude-code-{t}"),
            BINARY.to_vec(),
        ),
    ])
}

/// That release, sound.
pub(crate) fn good_release(base: &str) -> std::collections::BTreeMap<String, Vec<u8>> {
    release(base, 1, &fleet_core::adapters::sha256_hex(BINARY))
}

/// tmux, in memory: a launch adds a live pane, whose id it answers with.
#[derive(Default)]
pub(crate) struct FakeMux {
    /// The panes on the server.
    pub(crate) panes: Mutex<Vec<Pane>>,
    /// Whether a launch fails.
    pub(crate) fail_launch: bool,
    /// Whether ending a session fails.
    pub(crate) fail_stop: AtomicBool,
    /// Like a tmux that is not installed.
    pub(crate) missing: AtomicBool,
    /// Each paste: the pane and the text.
    pub(crate) typed: Mutex<Vec<(String, String)>>,
    /// Whether typing in fails.
    pub(crate) fail_type: AtomicBool,
    /// What each launch ran, in order.
    pub(crate) launched: Mutex<Vec<Launched>>,
    /// How long each launch takes.
    pub(crate) launch_delay: std::time::Duration,
}

/// What running a tmux that is not installed comes to.
pub(crate) fn not_installed() -> ToolError {
    ToolError::CouldNotRun {
        program: "tmux".into(),
        error: std::io::Error::from(std::io::ErrorKind::NotFound),
    }
}

impl Multiplexer for FakeMux {
    fn launch(&self, session: &str, _: &Path, program: &Launched) -> Result<String, ToolError> {
        if self.missing.load(Ordering::Relaxed) {
            return Err(not_installed());
        }
        self.launched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(program.clone());
        std::thread::sleep(self.launch_delay);
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
            id: format!("%{pid}"),
            pid,
            dead: false,
        });
        Ok(format!("%{pid}"))
    }

    fn panes(&self) -> Result<Vec<Pane>, ToolError> {
        if self.missing.load(Ordering::Relaxed) {
            return Err(not_installed());
        }
        Ok(self
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone())
    }

    fn stop(&self, session: &str) -> Result<(), ToolError> {
        if self.fail_stop.load(Ordering::Relaxed) {
            return Err(ToolError::Failed {
                command: "tmux kill-session".into(),
                said: "no".into(),
            });
        }
        self.panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|p| p.session != session);
        Ok(())
    }

    fn type_in(&self, pane: &str, text: &str) -> Result<(), ToolError> {
        if self.missing.load(Ordering::Relaxed) {
            return Err(not_installed());
        }
        if self.fail_type.load(Ordering::Relaxed) {
            return Err(ToolError::Failed {
                command: "tmux paste-buffer".into(),
                said: "no".into(),
            });
        }
        self.typed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push((pane.into(), text.into()));
        Ok(())
    }

    fn end_all(&self) -> Result<(), ToolError> {
        self.panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        Ok(())
    }
}

/// Agent adapters in memory: a launch runs `/bin/agent`, a read finds
/// every asked session idle, and each speaks protocol 1, unless told
/// otherwise.
#[derive(Default)]
pub(crate) struct FakeAgents {
    /// The adapter of each launch, in order.
    pub(crate) launches: Mutex<Vec<PathBuf>>,
    /// The adapter and the sessions of each read, in order.
    pub(crate) reads: Mutex<Vec<(PathBuf, Vec<SessionRef>)>>,
    /// How every launch fails, if it does.
    pub(crate) launch_fault: Option<AdapterFault>,
    /// How every read fails, if it does.
    pub(crate) read_fault: Option<AdapterFault>,
    /// Each adapter's answer to a read, verbatim; one it does not hold
    /// answers with no readings. Unset, every asked session reads
    /// `activity`.
    pub(crate) readings: Mutex<Option<BTreeMap<PathBuf, Vec<Reading>>>>,
    /// What every asked session reads while `readings` is unset; idle
    /// until set.
    pub(crate) activity: Mutex<Option<Activity>>,
    /// What every adapter says it speaks; unset, protocol 1.
    pub(crate) speaks: Option<Result<u32, AdapterFault>>,
    /// How long each launch takes.
    pub(crate) launch_delay: std::time::Duration,
}

impl FakeAgents {
    /// Every asked session reads `activity` from now on.
    pub(crate) fn read_as(&self, activity: Activity) {
        *self.activity.lock().unwrap_or_else(PoisonError::into_inner) = Some(activity);
    }
}

impl AgentAdapters for FakeAgents {
    fn launch(&self, adapter: &Path, _: &agent::Launch) -> Result<Launched, AdapterFault> {
        self.launches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(adapter.to_path_buf());
        std::thread::sleep(self.launch_delay);
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
            .push((adapter.to_path_buf(), request.sessions.clone()));
        if let Some(fault) = &self.read_fault {
            return Err(fault.clone());
        }
        let readings = self.readings.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(readings) = readings.as_ref() {
            return Ok(Readings {
                sessions: readings.get(adapter).cloned().unwrap_or_default(),
            });
        }
        let activity = self
            .activity
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .unwrap_or(Activity::Idle);
        Ok(Readings {
            sessions: request
                .sessions
                .iter()
                .map(|s| Reading {
                    session: s.session.clone(),
                    activity,
                    reason: None,
                })
                .collect(),
        })
    }

    fn protocol(&self, _: &Path) -> Result<Speaks, AdapterFault> {
        self.speaks
            .clone()
            .unwrap_or(Ok(1))
            .map(|protocol| Speaks { protocol })
    }
}

/// Agent adapters a test keeps a handle on, to see what was asked of them.
pub(crate) struct Shared(pub(crate) Arc<FakeAgents>);
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

/// tmux a test keeps a handle on, to see and change its panes.
pub(crate) struct SharedMux(pub(crate) Arc<FakeMux>);
impl Multiplexer for SharedMux {
    fn launch(&self, s: &str, d: &Path, l: &Launched) -> Result<String, ToolError> {
        self.0.launch(s, d, l)
    }
    fn panes(&self) -> Result<Vec<Pane>, ToolError> {
        self.0.panes()
    }
    fn stop(&self, s: &str) -> Result<(), ToolError> {
        self.0.stop(s)
    }
    fn type_in(&self, p: &str, t: &str) -> Result<(), ToolError> {
        self.0.type_in(p, t)
    }
    fn end_all(&self) -> Result<(), ToolError> {
        self.0.end_all()
    }
}

/// A network a test keeps a handle on, to see what was fetched.
pub(crate) struct SharedNet(pub(crate) Arc<FakeNet>);
impl fleet_core::tools::Network for SharedNet {
    fn download(&self, u: &str, to: &Path) -> Result<fleet_core::tools::Fetched, ToolError> {
        self.0.download(u, to)
    }
    fn version(&self) -> Result<String, ToolError> {
        self.0.version()
    }
}

/// The supervisor's tools made of `mux`, `agents` and `net`, which the
/// test keeps.
pub(crate) fn tools(mux: Arc<FakeMux>, agents: Arc<FakeAgents>, net: Arc<FakeNet>) -> Tools {
    Tools {
        mux: Box::new(SharedMux(mux)),
        agents: Box::new(Shared(agents)),
        net: Box::new(SharedNet(net)),
    }
}

/// The adapter store of a `tree` rooted at `root`, fetching from a base no
/// test serves.
pub(crate) fn store(root: &Path) -> Store {
    Store::new(&root.join("st"), Some("https://r".into()))
}

/// Every pane on `mux` ended, as when its agent exits.
pub(crate) fn kill_all(mux: &FakeMux) {
    for p in mux
        .panes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter_mut()
    {
        p.dead = true;
    }
}

/// Each paste on `mux`: the pane and the text.
pub(crate) fn typed(mux: &FakeMux) -> Vec<(String, String)> {
    mux.typed
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The kinds of the steps recorded, oldest first.
pub(crate) fn steps(sup: &Supervisor) -> Vec<String> {
    sup.lock().steps().unwrap_or_default()
}

/// The messages held for delivery.
pub(crate) fn held(sup: &Supervisor) -> Vec<MessageRow> {
    sup.lock().held().unwrap_or_default()
}
