//! `fleet stop`: every project off, the supervisor down and disabled.

use std::path::Path;
use std::time::Duration;

use fleet_core::tools::ServiceManager;

use crate::client;
use crate::protocol::{Doing, Doubt, Problem, Response, Status};
use crate::ui::{self, words};

pub(crate) fn run(state_dir: &Path, service: &dyn ServiceManager) -> Result<Response, Status> {
    run_within(state_dir, service, client::WAIT)
}

/// `run`, waiting at most `wait` for the supervisor at each step.
fn run_within(
    state_dir: &Path,
    service: &dyn ServiceManager,
    wait: Duration,
) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    let mut answers = client::answers(&socket);
    // Running but silent: it may be starting, so give it the time start does.
    let silent = !answers && service.running(state_dir);
    if silent {
        let spinner = ui::spinner(words::WAITING);
        answers = client::wait_until(&socket, true, wait);
        spinner.clear();
    }
    if answers {
        client::stop_all(&socket)?;
    }
    service.stop_and_disable(state_dir).map_err(|error| {
        Status::Cannot(Problem::ServiceManager {
            doing: Doing::Stop,
            error,
        })
    })?;
    if !answers {
        // Only the supervisor writes state, so its fleets stay as they were.
        return if silent {
            Err(Status::CouldNotTell(Doubt::StoppedUnanswered {
                waited: wait,
                log: service.where_logs_go(state_dir),
            }))
        } else {
            Ok(Response::SupervisorWasNotRunning)
        };
    }
    let spinner = ui::spinner(words::STOPPING);
    let gone = client::wait_until(&socket, false, wait);
    spinner.clear();
    if gone {
        Ok(Response::SupervisorStopped)
    } else {
        Err(Status::CouldNotTell(Doubt::StillAnswers { waited: wait }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;
    use std::path::PathBuf;
    use std::time::Duration;

    use fleet_core::tools::{SupervisorUnit, ToolError};

    /// A service manager whose supervisor never answers on the socket.
    struct Silent {
        running: bool,
        stopped: Cell<bool>,
    }
    impl ServiceManager for Silent {
        fn enable_and_start(&self, _: &SupervisorUnit) -> Result<(), ToolError> {
            Ok(())
        }
        fn stop_and_disable(&self, _: &Path) -> Result<(), ToolError> {
            self.stopped.set(true);
            Ok(())
        }
        fn running(&self, _: &Path) -> bool {
            self.running
        }
        fn where_logs_go(&self, _: &Path) -> String {
            "its.log".into()
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

    fn stop(name: &str, running: bool) -> (Result<Response, Status>, bool) {
        let dir = std::env::temp_dir().join(format!("fleet-{name}-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let service = Silent {
            running,
            stopped: Cell::new(false),
        };
        let ending = run_within(&dir, &service, Duration::from_millis(200));
        let _ = fs::remove_dir_all(dir);
        (ending, service.stopped.get())
    }

    #[test]
    fn a_running_supervisor_that_never_answers_is_stopped_and_its_fleets_left_as_they_were() {
        let (ending, stopped) = stop("stop-silent", true);
        assert!(stopped);
        assert!(
            matches!(
                &ending,
                Err(Status::CouldNotTell(Doubt::StoppedUnanswered { log, .. })) if log == "its.log"
            ),
            "{ending:?}"
        );
    }

    #[test]
    fn with_nothing_running_the_supervisor_was_not_running() {
        let (ending, stopped) = stop("stop-none", false);
        assert!(stopped);
        assert!(
            matches!(ending, Ok(Response::SupervisorWasNotRunning)),
            "{ending:?}"
        );
    }
}
