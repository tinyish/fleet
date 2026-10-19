//! What `fleet uninstall` says.

use std::io;
use std::path::Path;

use crate::ui::output::Ink;

/// What `fleet uninstall --help` says uninstall does.
pub(crate) const ABOUT: &str =
    "Take tinyfleet off this machine: the supervisor, its state and this fleet binary";

/// Said when uninstall would ask and there is no terminal.
pub(super) const NEEDS_ANSWER: &str = "uninstall asks first, and there is no terminal to ask";

/// The question uninstall asks first, naming the binary it deletes.
pub(crate) fn prompt(binary: &Path) -> String {
    format!(
        "Uninstall tinyfleet? This removes the supervisor, its state and {}",
        binary.display()
    )
}

/// What uninstall did.
pub(super) fn uninstalled(binary: &Path, state_dir: &Path, kept: bool, ink: Ink) -> String {
    let done = format!(
        "tinyfleet is uninstalled: the supervisor, its state and {} are gone",
        ink.path(binary)
    );
    if !kept {
        return done;
    }
    format!(
        "{done}; {} holds files tinyfleet did not write, so it was kept",
        ink.path(state_dir)
    )
}

/// How to uninstall with nobody there to answer.
pub(super) fn add_flag(ink: Ink) -> String {
    format!(
        "add {} to uninstall without asking",
        ink.accent("--non-interactive")
    )
}

/// A file uninstall could not remove, after the supervisor was stopped.
pub(super) fn could_not_remove(path: &Path, error: &io::Error, ink: Ink) -> String {
    format!(
        "the supervisor is stopped, but {} could not be removed: {error}",
        ink.path(path)
    )
}

/// The way on after `could_not_remove`.
pub(super) fn try_again(ink: Ink) -> String {
    format!("delete it, or run {} again", ink.accent("fleet uninstall"))
}
