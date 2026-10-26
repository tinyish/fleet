//! What `fleet create` says when it is done.

use fleet_core::protocol::response;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::OUT;
    use std::path::PathBuf;

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
}
