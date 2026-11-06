//! `fleet spawn`: a session from a role of the fleet here, in the role's
//! directory or one of the fleet's projects by its root, answered with its
//! id. It never starts the supervisor: starting it would turn every project
//! on.

use std::path::Path;

use fleet_core::protocol::{self, request};

use crate::client;
use crate::protocol::{Response, Status};
use crate::ui::{self, words};

/// Spawns `role` from the current directory, in `directory` if named.
pub(crate) fn run(
    state_dir: &Path,
    role: &str,
    directory: Option<&Path>,
) -> Result<Response, Status> {
    let (supervisor, here) = client::from_here(state_dir)?;
    let request = protocol::Request::Spawn(request_for(&here, role, directory));
    let spinner = ui::spinner(words::SPAWNING);
    let spawned = supervisor.ask(&request, |answer| match answer {
        protocol::Response::Spawn(spawned) => Some(spawned),
        _ => None,
    });
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
}
