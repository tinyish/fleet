//! What the supervisor's tests share.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use fleet_core::agent::{
    self, Activity, AdapterFault, Launched, Reading, Readings, SessionRef, Speaks,
};
use fleet_core::tools::{AgentAdapters, Multiplexer, Pane, ToolError};

use state::State;

/// A temp tree with repositories `names` under `code/`, a `wing/`
/// directory, and a state file under `st/`, all canonical.
pub(crate) fn tree(name: &str, names: &[&str]) -> (PathBuf, Mutex<State>) {
    let root = std::env::temp_dir().join(format!("fleet-sup-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for repo in names {
        let _ = fs::create_dir_all(root.join("code").join(repo).join(".git"));
    }
    let _ = fs::create_dir_all(root.join("wing"));
    let _ = fs::create_dir_all(root.join("st"));
    let root = fs::canonicalize(&root).unwrap_or(root);
    let state = State::open(&root.join("st")).ok();
    assert!(state.is_some());
    (root, Mutex::new(state.unwrap_or_else(|| unreachable!())))
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
    /// Like a tmux that is not installed.
    pub(crate) missing: bool,
}

/// What running a tmux that is not installed comes to.
pub(crate) fn not_installed() -> ToolError {
    ToolError::CouldNotRun {
        program: "tmux".into(),
        error: std::io::Error::from(std::io::ErrorKind::NotFound),
    }
}

impl Multiplexer for FakeMux {
    fn launch(&self, session: &str, _: &Path, _: &Launched) -> Result<String, ToolError> {
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
            id: format!("%{pid}"),
            pid,
            dead: false,
        });
        Ok(format!("%{pid}"))
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

/// Agent adapters in memory: a launch runs `/bin/agent`, and a read finds
/// every asked session idle, unless told otherwise.
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
    /// answers with no readings. Unset, every asked session is idle.
    pub(crate) readings: Option<BTreeMap<PathBuf, Vec<Reading>>>,
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
            .push((adapter.to_path_buf(), request.sessions.clone()));
        if let Some(fault) = &self.read_fault {
            return Err(fault.clone());
        }
        if let Some(readings) = &self.readings {
            return Ok(Readings {
                sessions: readings.get(adapter).cloned().unwrap_or_default(),
            });
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
pub(crate) struct Shared(pub(crate) std::sync::Arc<FakeAgents>);
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
