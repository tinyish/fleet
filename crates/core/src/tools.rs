//! The tool interfaces: one trait per kind of outside tool. Drivers
//! implement them; nothing in this crate runs a tool.

use std::io;
use std::path::{Path, PathBuf};

/// What the service manager needs to run the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorUnit {
    /// The `fleet` binary the unit runs, as `<program> supervise`.
    pub program: PathBuf,
    /// The state directory, passed to the supervisor as `FLEET_STATE_DIR`.
    pub state_dir: PathBuf,
}

/// An outside tool failed, as facts; the CLI words them.
#[derive(Debug)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(ToolErrorKind), derive(strum::EnumIter))
)]
pub enum ToolError {
    /// The installed unit serves another state directory, so it is left
    /// alone.
    OwnedElsewhere {
        /// The state directory the installed unit serves.
        theirs: PathBuf,
        /// The state directory this command serves.
        ours: PathBuf,
    },
    /// A program could not be started.
    CouldNotRun {
        /// The program.
        program: String,
        /// What the system said.
        error: io::Error,
    },
    /// A program ran and failed.
    Failed {
        /// The program and its arguments.
        command: String,
        /// What it wrote on stderr.
        said: String,
    },
    /// A file the tool needs could not be worked on.
    File {
        /// What was being done to it.
        doing: FileStep,
        /// The file or directory.
        path: PathBuf,
        /// What the system said.
        error: io::Error,
    },
    /// A path cannot go into the supervisor's unit file.
    UnitPath {
        /// Which path it is.
        which: UnitPath,
        /// The path.
        path: PathBuf,
        /// Why it cannot.
        why: BadUnitPath,
    },
}

/// What was being done to a file when it failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
pub enum FileStep {
    /// Creating a directory.
    Create,
    /// Making a directory private to the user.
    Secure,
    /// Writing a file.
    Write,
    /// Removing a file.
    Remove,
}

/// A path the supervisor's unit file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
pub enum UnitPath {
    /// The `fleet` binary.
    Program,
    /// The state directory.
    StateDir,
    /// The supervisor's log.
    Log,
}

/// Why a path cannot go into a unit file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
pub enum BadUnitPath {
    /// It is not valid UTF-8.
    NotUtf8,
    /// It contains a newline.
    Newline,
}

/// The service manager that runs the supervisor: launchd or systemd.
pub trait ServiceManager {
    /// Writes the supervisor's unit, enables it to start at login, and
    /// starts it (restarting it if it is loaded but not answering).
    ///
    /// # Errors
    ///
    /// When writing, loading or starting fails, and `OwnedElsewhere` when
    /// the installed unit serves another state directory.
    fn enable_and_start(&self, unit: &SupervisorUnit) -> Result<(), ToolError>;

    /// Stops the supervisor of `state_dir`, disables it, and removes its
    /// unit. A unit that is not installed is not an error.
    ///
    /// # Errors
    ///
    /// When stopping or removing fails, and `OwnedElsewhere` when the
    /// installed unit serves another state directory.
    fn stop_and_disable(&self, state_dir: &Path) -> Result<(), ToolError>;

    /// Where the supervisor's log can be read, for a person.
    fn where_logs_go(&self, state_dir: &Path) -> String;
}
