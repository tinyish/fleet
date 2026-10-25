//! What the supervisor's tests share.

use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

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
