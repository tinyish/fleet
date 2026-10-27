//! `fleet spawn`: a session from a role of the fleet here, in the role's
//! directory or one of the fleet's projects by its root, answered with its
//! id. It never starts the supervisor: starting it would turn every project
//! on.

use std::path::Path;

use fleet_core::protocol::request;

use crate::client;
use crate::protocol::{Problem, Response, Status};
use crate::ui::{self, words};

/// Spawns `role` from the current directory, in `directory` if named.
pub(crate) fn run(
    state_dir: &Path,
    role: &str,
    directory: Option<&Path>,
) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    if !client::answers(&socket) {
        return Err(Status::NotRunning);
    }
    let here = std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    let spinner = ui::spinner(words::SPAWNING);
    let spawned = client::spawn(&socket, request_for(&here, role, directory));
    spinner.clear();
    Ok(Response::Spawned(spawned?))
}

/// Spawn's request from `here`, a named directory made absolute from it.
fn request_for(here: &Path, role: &str, directory: Option<&Path>) -> request::Spawn {
    request::Spawn::Session {
        directory: here.to_path_buf(),
        role: role.to_owned(),
        working_directory: directory.map(|d| here.join(d)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn spawn_takes_a_role_and_maybe_a_directory() {
        let args =
            crate::Args::try_parse_from(["fleet", "spawn", "worker", "--directory", "../api"]);
        assert!(
            matches!(
                args.as_ref().map(|a| &a.request),
                Ok(crate::protocol::Request::Spawn { role, directory: Some(d) })
                    if role == "worker" && d.as_path() == Path::new("../api")
            ),
            "{args:?}"
        );
        assert!(crate::Args::try_parse_from(["fleet", "spawn"]).is_err());
    }

    #[test]
    fn the_directory_is_made_absolute_from_where_spawn_runs() {
        assert_eq!(
            request_for(Path::new("/c/wing"), "worker", Some(Path::new("../api"))),
            request::Spawn::Session {
                directory: "/c/wing".into(),
                role: "worker".into(),
                working_directory: Some("/c/wing/../api".into()),
            }
        );
        assert_eq!(
            request_for(Path::new("/c/wing"), "worker", Some(Path::new("/c/app"))),
            request::Spawn::Session {
                directory: "/c/wing".into(),
                role: "worker".into(),
                working_directory: Some("/c/app".into()),
            }
        );
        assert_eq!(
            request_for(Path::new("/c/wing"), "worker", None),
            request::Spawn::Session {
                directory: "/c/wing".into(),
                role: "worker".into(),
                working_directory: None,
            }
        );
    }

    #[test]
    fn spawn_never_starts_the_supervisor() {
        let dir = std::env::temp_dir().join(format!("fleet-spawn-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(run(&dir, "worker", None), Err(Status::NotRunning)));
        let socket = fleet_core::protocol::socket_path(&dir).ok();
        assert!(socket.is_some_and(|s| !s.exists()));
        let _ = std::fs::remove_dir_all(dir);
    }
}
