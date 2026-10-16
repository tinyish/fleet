//! The tool interfaces: one trait per kind of outside tool. Drivers
//! implement them; nothing in this crate runs a tool.

use std::fmt;
use std::path::{Path, PathBuf};

/// What the service manager needs to run the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorUnit {
    /// The `fleet` binary the unit runs, as `<program> supervise`.
    pub program: PathBuf,
    /// The state directory, passed to the supervisor as `FLEET_STATE_DIR`.
    pub state_dir: PathBuf,
}

/// An outside tool failed, with what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError(pub String);

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The service manager that runs the supervisor: launchd or systemd.
pub trait ServiceManager {
    /// Writes the supervisor's unit, enables it to start at login, and
    /// starts it (restarting it if it is loaded but not answering).
    ///
    /// # Errors
    ///
    /// The tool's own message when writing, loading or starting fails, and a
    /// refusal when the installed unit serves another state directory.
    fn enable_and_start(&self, unit: &SupervisorUnit) -> Result<(), ToolError>;

    /// Stops the supervisor of `state_dir`, disables it, and removes its
    /// unit. A unit that is not installed is not an error.
    ///
    /// # Errors
    ///
    /// The tool's own message when stopping or removing fails, and a refusal
    /// when the installed unit serves another state directory.
    fn stop_and_disable(&self, state_dir: &Path) -> Result<(), ToolError>;

    /// Where the supervisor's log can be read, for a person.
    fn where_logs_go(&self, state_dir: &Path) -> String;
}
