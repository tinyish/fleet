//! `fleet uninstall`: after asking, the supervisor stopped and its unit
//! removed, tinyfleet's own files in the state directory deleted, and this
//! `fleet` binary too. The `fleet.toml` files in projects stay.

use std::fs;
use std::io;
use std::path::Path;

use fleet_core::tools::ServiceManager;

use crate::Ask;
use crate::cmd::stop;
use crate::protocol::{Doubt, Problem, Response, Status};
use crate::ui::{self, words};

pub(crate) fn run(
    state_dir: &Path,
    service: &dyn ServiceManager,
    ask: Ask,
) -> Result<Response, Status> {
    let binary = std::env::current_exe().map_err(|e| Status::Cannot(Problem::NoBinary(e)))?;
    run_with(state_dir, service, ask, &binary)
}

/// `run`, deleting `binary` as this `fleet`.
fn run_with(
    state_dir: &Path,
    service: &dyn ServiceManager,
    ask: Ask,
    binary: &Path,
) -> Result<Response, Status> {
    match ask {
        Ask::Yes => {}
        Ask::Never => return Err(Status::NeedsAnswer),
        Ask::Terminal => {
            // A cancel is a no.
            if !ui::confirm(&words::uninstall::prompt(binary), false).unwrap_or(false) {
                return Ok(Response::Cancelled);
            }
        }
    }
    let stopped = stop::run(state_dir, service);
    if !supervisor_down(&stopped) {
        return stopped;
    }
    let kept_state_dir = remove_own_files(state_dir)?;
    remove(binary)?;
    Ok(Response::Uninstalled {
        binary: binary.to_path_buf(),
        state_dir: state_dir.to_path_buf(),
        kept_state_dir,
    })
}

/// Whether stop left the supervisor down, so its state may go: stopped, not
/// running, or stopped without answering. Any other ending (the unit serves
/// another directory, it still answers, the conversation failed) keeps
/// everything.
fn supervisor_down(stopped: &Result<Response, Status>) -> bool {
    matches!(
        stopped,
        Ok(Response::SupervisorStopped | Response::SupervisorWasNotRunning)
            | Err(Status::CouldNotTell(Doubt::StoppedUnanswered { .. }))
    )
}

/// Deletes tinyfleet's own files in `dir`, then `dir` if that empties it, so
/// a `FLEET_STATE_DIR` pointed at a shared directory loses nothing else.
/// Returns whether `dir` was kept because it holds other files.
fn remove_own_files(dir: &Path) -> Result<bool, Status> {
    for name in fleet_core::paths::OWN_FILES {
        remove(&dir.join(name))?;
    }
    // A state directory that is a symlink: the link goes, then its target
    // as any state directory would. A dangling link just goes.
    let mut dir = dir.to_path_buf();
    if fs::symlink_metadata(&dir).is_ok_and(|m| m.file_type().is_symlink()) {
        let target = fs::canonicalize(&dir).ok();
        remove(&dir)?;
        match target {
            Some(target) => dir = target,
            None => return Ok(false),
        }
    }
    match fs::remove_dir(&dir) {
        Ok(()) => Ok(false),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        // Finder's own bookkeeping, alone, is not the person's: it goes too.
        Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty && only_finder_left(&dir) => {
            remove(&dir.join(FINDER_FILE))?;
            fs::remove_dir(&dir).map_err(|error| could_not(&dir, error))?;
            Ok(false)
        }
        Err(e) if e.kind() == io::ErrorKind::DirectoryNotEmpty => Ok(true),
        Err(error) => Err(could_not(&dir, error)),
    }
}

/// The file macOS's Finder leaves in a directory it has shown.
const FINDER_FILE: &str = ".DS_Store";

/// Whether Finder's file is all that is left in `dir`.
fn only_finder_left(dir: &Path) -> bool {
    fs::read_dir(dir).is_ok_and(|entries| {
        let names: Vec<_> = entries
            .filter_map(Result::ok)
            .map(|e| e.file_name())
            .collect();
        names == [FINDER_FILE]
    })
}

/// Deletes one file; one already gone is fine.
fn remove(path: &Path) -> Result<(), Status> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(could_not(path, error)),
    }
}

fn could_not(path: &Path, error: io::Error) -> Status {
    Status::Cannot(Problem::Uninstall {
        path: path.to_path_buf(),
        error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::path::PathBuf;

    use fleet_core::paths::OWN_FILES;
    use fleet_core::tools::{SupervisorUnit, ToolError};

    struct Fake {
        stopped: Cell<bool>,
        fail_stop: bool,
    }
    impl ServiceManager for Fake {
        fn enable_and_start(&self, _: &SupervisorUnit) -> Result<(), ToolError> {
            Ok(())
        }
        fn stop_and_disable(&self, state_dir: &Path) -> Result<(), ToolError> {
            if self.fail_stop {
                return Err(ToolError::OwnedElsewhere {
                    theirs: "/elsewhere".into(),
                    ours: state_dir.to_path_buf(),
                });
            }
            self.stopped.set(true);
            Ok(())
        }
        fn running(&self, _: &Path) -> bool {
            false
        }
        fn where_logs_go(&self, _: &Path) -> String {
            String::new()
        }
        fn name(&self) -> &'static str {
            "launchd"
        }
        fn answers(&self) -> Result<(), ToolError> {
            Ok(())
        }
        fn installed_state_dir(&self) -> Option<PathBuf> {
            None
        }
    }

    fn fake(fail_stop: bool) -> Fake {
        Fake {
            stopped: Cell::new(false),
            fail_stop,
        }
    }

    /// A temp dir with `state/` holding every file tinyfleet keeps, and a
    /// stand-in binary `bin/fleet`.
    fn machine(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("fleet-un-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let state = root.join("state");
        let _ = fs::create_dir_all(&state);
        for file in OWN_FILES {
            let _ = fs::write(state.join(file), "x");
        }
        let _ = fs::create_dir_all(root.join("bin"));
        let binary = root.join("bin").join("fleet");
        let _ = fs::write(&binary, "binary");
        (root, state, binary)
    }

    fn all_there(state: &Path, binary: &Path) -> bool {
        OWN_FILES.iter().all(|f| state.join(f).exists()) && binary.exists()
    }

    #[test]
    fn ask_never_removes_nothing_and_needs_an_answer() {
        let (root, state, binary) = machine("never");
        let service = fake(false);
        let ending = run_with(&state, &service, Ask::Never, &binary);
        assert!(matches!(ending, Err(Status::NeedsAnswer)), "{ending:?}");
        assert!(all_there(&state, &binary));
        assert!(!service.stopped.get());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn non_interactive_removes_tinyfleets_files_the_empty_dir_and_the_binary() {
        let (root, state, binary) = machine("yes");
        let service = fake(false);
        let ending = run_with(&state, &service, Ask::Yes, &binary);
        assert!(
            matches!(
                ending,
                Ok(Response::Uninstalled {
                    kept_state_dir: false,
                    ..
                })
            ),
            "{ending:?}"
        );
        assert!(!state.exists() && !binary.exists());
        assert!(service.stopped.get());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_state_dir_holding_other_files_is_kept_and_said() {
        let (root, state, binary) = machine("kept");
        let _ = fs::write(state.join("notes.txt"), "mine");
        let ending = run_with(&state, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(
                ending,
                Ok(Response::Uninstalled {
                    kept_state_dir: true,
                    ..
                })
            ),
            "{ending:?}"
        );
        assert!(state.join("notes.txt").exists());
        assert!(OWN_FILES.iter().all(|f| !state.join(f).exists()));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_stop_that_fails_removes_nothing() {
        let (root, state, binary) = machine("fails");
        let ending = run_with(&state, &fake(true), Ask::Yes, &binary);
        assert!(
            matches!(ending, Err(Status::Cannot(Problem::ServiceManager { .. }))),
            "{ending:?}"
        );
        assert!(all_there(&state, &binary));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn again_after_everything_is_gone_still_succeeds() {
        let (root, state, binary) = machine("again");
        let _ = run_with(&state, &fake(false), Ask::Yes, &binary);
        let ending = run_with(&state, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(ending, Ok(Response::Uninstalled { .. })),
            "{ending:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_binary_that_cannot_be_removed_is_named_and_a_rerun_finishes() {
        let (root, state, binary) = machine("binary");
        let _ = fs::remove_file(&binary);
        let _ = fs::create_dir_all(binary.join("inside"));
        let ending = run_with(&state, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(&ending, Err(Status::Cannot(Problem::Uninstall { path, .. })) if *path == binary),
            "{ending:?}"
        );
        assert!(!state.exists());
        let _ = fs::remove_dir_all(&binary);
        let _ = fs::write(&binary, "binary");
        let ending = run_with(&state, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(ending, Ok(Response::Uninstalled { .. })),
            "{ending:?}"
        );
        assert!(!binary.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn only_some_files_left_is_fine() {
        let (root, state, binary) = machine("some");
        for file in OWN_FILES.iter().filter(|f| **f != "supervisor.log") {
            let _ = fs::remove_file(state.join(file));
        }
        let ending = run_with(&state, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(ending, Ok(Response::Uninstalled { .. })),
            "{ending:?}"
        );
        assert!(!state.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_state_dir_that_is_a_symlink_is_emptied_and_unlinked() {
        let (root, real, binary) = machine("symlink");
        let link = root.join("link");
        let _ = std::os::unix::fs::symlink(&real, &link);
        let ending = run_with(&link, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(
                ending,
                Ok(Response::Uninstalled {
                    kept_state_dir: false,
                    ..
                })
            ),
            "{ending:?}"
        );
        assert!(fs::symlink_metadata(&link).is_err(), "the link is gone");
        assert!(!real.exists(), "the emptied target is gone");
        assert!(!binary.exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn only_a_supervisor_that_is_down_lets_the_state_go() {
        use std::time::Duration;
        let wait = Duration::from_secs(10);
        assert!(supervisor_down(&Ok(Response::SupervisorStopped)));
        assert!(supervisor_down(&Ok(Response::SupervisorWasNotRunning)));
        assert!(supervisor_down(&Err(Status::CouldNotTell(
            Doubt::StoppedUnanswered {
                waited: wait,
                log: String::new(),
            }
        ))));
        assert!(!supervisor_down(&Err(Status::CouldNotTell(
            Doubt::StillAnswers { waited: wait }
        ))));
        assert!(!supervisor_down(&Err(Status::CouldNotTell(
            Doubt::OddReply { to: "stop" }
        ))));
        assert!(!supervisor_down(&Err(Status::NotRunning)));
    }

    #[test]
    fn a_ds_store_left_alone_goes_and_one_beside_other_files_stays() {
        let (root, state, binary) = machine("ds-store");
        let _ = fs::write(state.join(".DS_Store"), "finder");
        let ending = run_with(&state, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(
                ending,
                Ok(Response::Uninstalled {
                    kept_state_dir: false,
                    ..
                })
            ),
            "{ending:?}"
        );
        assert!(!state.exists());
        let _ = fs::remove_dir_all(&root);
        let (root, state, binary) = machine("ds-store-kept");
        let _ = fs::write(state.join(".DS_Store"), "finder");
        let _ = fs::write(state.join("notes.txt"), "mine");
        let ending = run_with(&state, &fake(false), Ask::Yes, &binary);
        assert!(
            matches!(
                ending,
                Ok(Response::Uninstalled {
                    kept_state_dir: true,
                    ..
                })
            ),
            "{ending:?}"
        );
        assert!(state.join(".DS_Store").exists() && state.join("notes.txt").exists());
        let _ = fs::remove_dir_all(root);
    }
}
