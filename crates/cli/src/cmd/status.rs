//! `fleet status`: the seats of the fleet here, or of every fleet, and what
//! each agent is doing. It never starts the supervisor: looking would turn
//! projects on and fill seats.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::client;
use crate::protocol::{Problem, Response, Status};

/// Asks the supervisor what is running, from the current directory; with
/// the supervisor down, says so and starts nothing.
pub(crate) fn run(state_dir: &Path) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    if !client::answers(&socket) {
        return Err(Status::NotRunning);
    }
    let directory =
        std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    let overview = client::overview(&socket, directory)?;
    Ok(Response::Overview {
        overview,
        now_millis: now_millis(),
    })
}

/// Now, by the CLI's clock, in milliseconds since the Unix epoch: the
/// supervisor's clock, on the same machine.
fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_never_starts_the_supervisor() {
        let dir = std::env::temp_dir().join(format!("fleet-status-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(run(&dir), Err(Status::NotRunning)));
        let socket = fleet_core::protocol::socket_path(&dir).ok();
        assert!(socket.is_some_and(|s| !s.exists()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
