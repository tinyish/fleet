//! The words for where a fleet can go: create's and remove's refusals.

use fleet_core::status::Fleet;

use crate::ui::output::Ink;

/// Why a fleet or a project cannot go where it was asked.
pub(super) fn refused(refusal: &Fleet, ink: Ink) -> String {
    match refusal {
        Fleet::DirectoryNotRepoRoot { path, root } => format!(
            "{} is inside the repository at {}",
            ink.path(path),
            ink.path(root)
        ),
        Fleet::DirectoryNotInRepo { path } | Fleet::ProjectNotInRepo { path } => {
            format!("{} is not in a git repository", ink.path(path))
        }
        Fleet::ProjectNotRepoRoot { path, .. } => {
            format!("{} is not a repository's root", ink.path(path))
        }
        Fleet::ProjectIsFleetDirectory { path } => {
            format!("{} is the fleet's own directory", ink.path(path))
        }
        Fleet::WingmanInsideProject { directory, project } => format!(
            "a wingman fleet's directory must be outside the repositories it serves; \
             {} is inside {}",
            ink.path(directory),
            ink.path(project)
        ),
        Fleet::AlreadyHereServingOthers { serving, asked, .. } => format!(
            "a fleet is already here, serving {}; asked for {}",
            ink.paths(serving),
            ink.paths(asked)
        ),
        Fleet::InsideFleet { directory, fleet } | Fleet::NotFleetDirectory { directory, fleet } => {
            format!(
                "{} is inside the fleet in {}",
                ink.path(directory),
                ink.path(fleet)
            )
        }
        Fleet::ContainsFleet { directory, fleet } => format!(
            "{} contains the fleet in {}",
            ink.path(directory),
            ink.path(fleet)
        ),
        Fleet::InsideServedProject {
            directory,
            project,
            fleet,
        } => format!(
            "{} is inside {}, which the fleet in {} serves",
            ink.path(directory),
            ink.path(project),
            ink.path(fleet)
        ),
        Fleet::AlreadyServed { project, fleet } => format!(
            "{} is already served by the fleet in {}",
            ink.path(project),
            ink.path(fleet)
        ),
        Fleet::AlsoClaimed { project, fleet } => format!(
            "{} is also claimed by the fleet in {}",
            ink.path(project),
            ink.path(fleet)
        ),
    }
}

/// What to do about it, when there is something to do.
pub(super) fn hint(refusal: &Fleet, ink: Ink) -> Option<String> {
    let create = ink.accent("fleet create");
    Some(match refusal {
        Fleet::DirectoryNotInRepo { .. } => format!("run {create} at a repository's root"),
        Fleet::DirectoryNotRepoRoot { root, .. } => {
            format!("run {create} in {}", ink.whole_path(root))
        }
        Fleet::ProjectNotRepoRoot { root, .. } => format!(
            "pass the repository's root, {}, instead",
            ink.whole_path(root)
        ),
        Fleet::ProjectIsFleetDirectory { .. } => format!(
            "run {create} without {} to make a fleet of this repository itself",
            ink.accent("--project")
        ),
        Fleet::NotFleetDirectory { fleet, .. } => format!(
            "run {} in {}",
            ink.accent("fleet remove"),
            ink.whole_path(fleet)
        ),
        Fleet::ProjectNotInRepo { .. }
        | Fleet::WingmanInsideProject { .. }
        | Fleet::AlreadyHereServingOthers { .. }
        | Fleet::InsideFleet { .. }
        | Fleet::ContainsFleet { .. }
        | Fleet::InsideServedProject { .. }
        | Fleet::AlreadyServed { .. }
        | Fleet::AlsoClaimed { .. } => return None,
    })
}
