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
