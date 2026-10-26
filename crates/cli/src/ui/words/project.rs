//! The words for turning one project on or off.

use fleet_core::status::Project;

use crate::ui::output::Ink;

/// Why one project could not be turned on or off.
pub(super) fn refused(refusal: &Project, ink: Ink) -> String {
    match refusal {
        Project::NotServed { path } => format!("no fleet serves {}", ink.path(path)),
        Project::InsideProject { path, project } => format!(
            "{} is inside the project {}",
            ink.path(path),
            ink.path(project)
        ),
        Project::NoLongerServed { project, fleet } => format!(
            "the fleet in {} no longer serves {}: its {} dropped it",
            ink.path(fleet),
            ink.path(project),
            ink.accent("fleet.toml")
        ),
        Project::CannotTurnOn {
            project,
            fleet,
            why,
        } => format!(
            "{} cannot be turned on: the config of the fleet in {} cannot be used: {}",
            ink.path(project),
            ink.path(fleet),
            super::refused(why, ink)
        ),
    }
}

/// What to do about it, when there is something to do.
pub(super) fn hint(refusal: &Project, ink: Ink) -> Option<String> {
    Some(match refusal {
        Project::InsideProject { project, .. } => {
            format!("name its root, {}", ink.whole_path(project))
        }
        Project::CannotTurnOn { fleet, .. } => format!(
            "fix the {} in {}, then try again",
            ink.accent("fleet.toml"),
            ink.path(fleet)
        ),
        Project::NotServed { .. } | Project::NoLongerServed { .. } => return None,
    })
}
