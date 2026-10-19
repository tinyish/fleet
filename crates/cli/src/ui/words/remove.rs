//! What `fleet remove` says, and the line for a fleet that start forgot.

use std::path::Path;

use fleet_core::protocol::response::{self, ConfigFate};

use crate::ui::output::Ink;

/// What `fleet remove --help` says remove does.
pub(crate) const ABOUT: &str = "Remove the fleet here from tinyfleet, or take --project \
     repositories out of this wingman fleet";

/// The help for `fleet remove --project`.
pub(crate) const PROJECT_HELP: &str = "A repository to take out of this wingman fleet; repeat \
     for more. Taking its last one removes the fleet";

/// Asked before a whole fleet goes, when its `fleet.toml` is there.
pub(crate) const DELETE_PROMPT: &str = "Delete fleet.toml too?";

/// What the supervisor said remove did.
pub(crate) fn removed(remove: &response::Remove, ink: Ink) -> String {
    match remove {
        response::Remove::Removed {
            directory,
            projects,
            config,
        } => {
            let fleet = if projects.len() == 1 && projects[0] == *directory {
                "the fleet"
            } else {
                "the wingman fleet"
            };
            let file = ink.accent("fleet.toml");
            let after = match config {
                ConfigFate::Deleted => format!("; {file} was deleted"),
                ConfigFate::Kept => format!("; {file} was kept"),
                ConfigFate::Missing => String::new(),
            };
            format!("removed {fleet} in {}{after}", ink.path(directory))
        }
        response::Remove::ProjectsTaken {
            directory, taken, ..
        } => format!(
            "{} {} no longer served by the wingman fleet in {}",
            ink.paths(taken),
            if taken.len() == 1 { "is" } else { "are" },
            ink.path(directory)
        ),
        // The CLI asks and sends the request again, so a command never ends
        // here unless the supervisor asks twice.
        response::Remove::Ask { directory } => format!(
            "the fleet in {} was not removed: the supervisor asked about its {} again",
            ink.path(directory),
            ink.accent("fleet.toml")
        ),
        response::Remove::NothingHere { directory } => format!(
            "there is no fleet in {}; nothing was removed",
            ink.path(directory)
        ),
    }
}

/// A fleet that start forgot because its directory is gone.
pub(crate) fn forgotten(directory: &Path, ink: Ink) -> String {
    format!(
        "the fleet in {} was forgotten: its directory no longer exists",
        ink.path(directory)
    )
}
