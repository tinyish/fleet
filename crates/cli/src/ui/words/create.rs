//! What `fleet create` says when it is done, and when a `fleet.toml` is
//! already there.

use std::path::{Path, PathBuf};

use fleet_core::protocol::{self, response};

use crate::ui::output::Ink;

/// What the supervisor said create did.
pub(super) fn created(response: &response::Create, ink: Ink) -> String {
    let response::Create {
        directory,
        projects,
        created,
        adopted,
    } = response;
    if !created {
        return format!(
            "a fleet is already here, serving {}; nothing changed",
            ink.paths(projects)
        );
    }
    let default = projects.len() == 1 && projects[0] == *directory;
    if *adopted {
        return if default {
            format!("restored existing fleet in {}", ink.path(directory))
        } else {
            format!(
                "restored existing wingman fleet in {} serving {}",
                ink.path(directory),
                ink.paths(projects)
            )
        };
    }
    if default {
        return format!(
            "created a fleet in {} -- welcome aboard",
            ink.path(directory),
        );
    }
    format!(
        "created a wingman fleet in {} serving {}",
        ink.path(directory),
        ink.paths(projects)
    )
}

/// A `fleet.toml` no fleet owns lists other projects than those asked for.
pub(super) fn config_differs(path: &Path, listed: &[PathBuf], ink: Ink) -> String {
    format!(
        "{} is already here, serving {}",
        ink.path(path),
        ink.paths(listed)
    )
}

/// The way out of `config_differs`: take the file up as it is.
pub(super) fn take_it_up(ink: Ink) -> String {
    format!(
        "run {} without {} to take it up as it is",
        ink.accent("fleet create"),
        ink.accent("--project")
    )
}

/// Why a fleet or a project cannot go where it was asked: inside, around or
/// on top of another fleet. None for any other refusal.
pub(super) fn placement(status: &protocol::Status, ink: Ink) -> Option<String> {
    Some(match status {
        protocol::Status::InsideFleet { directory, fleet }
        | protocol::Status::NotFleetDirectory { directory, fleet } => format!(
            "{} is inside the fleet in {}",
            ink.path(directory),
            ink.path(fleet)
        ),
        protocol::Status::ContainsFleet { directory, fleet } => format!(
            "{} contains the fleet in {}",
            ink.path(directory),
            ink.path(fleet)
        ),
        protocol::Status::InsideServedProject {
            directory,
            project,
            fleet,
        } => format!(
            "{} is inside {}, which the fleet in {} serves",
            ink.path(directory),
            ink.path(project),
            ink.path(fleet)
        ),
        protocol::Status::AlreadyServed { project, fleet } => format!(
            "{} is already served by the fleet in {}",
            ink.path(project),
            ink.path(fleet)
        ),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::{ERR, OUT};

    fn plain(s: &str) -> String {
        console::strip_ansi_codes(s).into_owned()
    }

    fn create(directory: &str, projects: &[&str], adopted: bool) -> response::Create {
        response::Create {
            directory: directory.into(),
            projects: projects.iter().map(PathBuf::from).collect(),
            created: true,
            adopted,
        }
    }

    #[test]
    fn an_adopted_fleet_is_restored_and_a_new_one_still_welcomed() {
        assert_eq!(
            plain(&created(
                &create("/code/app", &["/code/app"], true),
                OUT.piped()
            )),
            "restored existing fleet in /code/app"
        );
        assert_eq!(
            plain(&created(
                &create("/code/wing", &["/code/api", "/code/web"], true),
                OUT.piped()
            )),
            "restored existing wingman fleet in /code/wing serving /code/api, /code/web"
        );
        assert_eq!(
            plain(&created(
                &create("/code/app", &["/code/app"], false),
                OUT.piped()
            )),
            "created a fleet in /code/app -- welcome aboard"
        );
    }

    #[test]
    fn a_differing_file_names_what_it_lists_and_how_to_take_it_up() {
        let listed = [PathBuf::from("/code/api"), PathBuf::from("/code/web")];
        assert_eq!(
            plain(&config_differs(
                Path::new("/code/wing/fleet.toml"),
                &listed,
                ERR.piped()
            )),
            "/code/wing/fleet.toml is already here, serving /code/api, /code/web"
        );
        assert_eq!(
            plain(&take_it_up(ERR.piped())),
            "run fleet create without --project to take it up as it is"
        );
    }
}
