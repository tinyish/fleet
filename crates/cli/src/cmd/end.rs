//! `fleet end`: ends a seat's session (the seat comes back fresh while its
//! fleet is on) or a session by its id, dropping the messages held for it.
//! It never starts the supervisor.

use std::path::Path;

use fleet_core::protocol::request;

use crate::client;
use crate::protocol::{Problem, Response, Status};

/// Ends `name`, a seat of the fleet here or a session's id.
pub(crate) fn run(state_dir: &Path, name: &str) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    if !client::answers(&socket) {
        return Err(Status::NotRunning);
    }
    let directory =
        std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    let ended = client::end(
        &socket,
        request::End::Session {
            directory,
            name: name.to_owned(),
        },
    )?;
    Ok(Response::Ended(ended))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn end_takes_one_name() {
        let args = crate::Args::try_parse_from(["fleet", "end", "scribe"]);
        assert!(
            matches!(
                args.as_ref().map(|a| &a.request),
                Ok(crate::protocol::Request::End { name }) if name == "scribe"
            ),
            "{args:?}"
        );
        assert!(crate::Args::try_parse_from(["fleet", "end"]).is_err());
    }

    #[test]
    fn end_never_starts_the_supervisor() {
        let dir = std::env::temp_dir().join(format!("fleet-end-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(run(&dir, "scribe"), Err(Status::NotRunning)));
        let socket = fleet_core::protocol::socket_path(&dir).ok();
        assert!(socket.is_some_and(|s| !s.exists()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
