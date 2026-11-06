//! `fleet status`: the seats of the fleet here, or of every fleet, and what
//! each agent is doing. It never starts the supervisor: looking would turn
//! projects on and fill seats.

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use fleet_core::protocol;

use crate::client;
use crate::protocol::{Response, Status};

/// Asks the supervisor what is running, from the current directory; with
/// the supervisor down, says so and starts nothing.
pub(crate) fn run(state_dir: &Path) -> Result<Response, Status> {
    let (supervisor, directory) = client::from_here(state_dir)?;
    let request = protocol::Request::Overview { directory };
    let overview = supervisor.ask(&request, |answer| match answer {
        protocol::Response::Overview(overview) => Some(overview),
        _ => None,
    })?;
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
