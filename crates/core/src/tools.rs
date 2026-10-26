//! The tool interfaces: one trait per kind of outside tool. Drivers
//! implement them; nothing in this crate runs a tool.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use crate::agent;

/// What the service manager needs to run the supervisor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorUnit {
    /// The `fleet` binary the unit runs, as `<program> supervise`.
    pub program: PathBuf,
    /// The state directory, passed to the supervisor as `FLEET_STATE_DIR`.
    pub state_dir: PathBuf,
    /// The `PATH` the supervisor, and the agents it launches, run with: the
    /// CLI's, so they find what the person's shell finds. None writes none.
    pub search_path: Option<OsString>,
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
    /// A command would be longer than the tool takes.
    TooLong {
        /// How long it would be, in bytes.
        bytes: usize,
        /// The most the tool takes, in bytes.
        limit: usize,
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
    /// Looking at a file or directory.
    Read,
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
    /// The `PATH` it runs with.
    SearchPath,
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

/// Why there is no service manager to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoServiceManager {
    /// tinyfleet does not run on this system.
    UnsupportedOs,
    /// `HOME` is not set, so its units cannot be found.
    NoHome,
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

    /// Whether the service manager is running the supervisor unit that
    /// serves `state_dir` (launchd: loaded; systemd: active), whether or not
    /// the supervisor answers. False when no unit is installed or it serves
    /// another state directory.
    fn running(&self, state_dir: &Path) -> bool;

    /// Where the supervisor's log can be read, for a person.
    fn where_logs_go(&self, state_dir: &Path) -> String;

    /// The service manager's name, as its own tool is called: `launchd` or
    /// `systemd`.
    fn name(&self) -> &'static str;

    /// Whether the service manager answers for this user.
    ///
    /// # Errors
    ///
    /// What went wrong when it does not answer.
    fn answers(&self) -> Result<(), ToolError>;

    /// The state directory the installed supervisor unit serves, or `None`
    /// when no unit is installed.
    fn installed_state_dir(&self) -> Option<PathBuf>;
}

/// git.
pub trait Git {
    /// git's version, as `git --version` says it.
    ///
    /// # Errors
    ///
    /// When git is not on the `PATH` (`CouldNotRun` with a not-found error)
    /// or does not run.
    fn version(&self) -> Result<String, ToolError>;
}

/// One pane on tinyfleet's tmux server: its session, its id, the agent's
/// process id, and whether the agent has exited (the pane stays, so its end
/// can be read).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    /// The session's name: tinyfleet's id for it.
    pub session: String,
    /// The pane's id, as tmux names it (`%12`): unique on its server, so it
    /// tells the agent's pane from one a person split off.
    pub id: String,
    /// The agent's process id.
    pub pid: u32,
    /// Whether the agent has exited.
    pub dead: bool,
}

/// The multiplexer that holds agents' sessions: tmux, on tinyfleet's own
/// server.
pub trait Multiplexer {
    /// Starts `program` in a new session named `session`, in `directory`,
    /// and answers with the id of the pane it runs in.
    ///
    /// # Errors
    ///
    /// When `directory` is not a directory, or the multiplexer fails.
    fn launch(
        &self,
        session: &str,
        directory: &Path,
        program: &agent::Launched,
    ) -> Result<String, ToolError>;

    /// Every pane on tinyfleet's server; none when the server is not
    /// running.
    ///
    /// # Errors
    ///
    /// When the multiplexer fails.
    fn panes(&self) -> Result<Vec<Pane>, ToolError>;

    /// Ends `session`. A session that is not there is not an error.
    ///
    /// # Errors
    ///
    /// When the multiplexer fails.
    fn stop(&self, session: &str) -> Result<(), ToolError>;
}

/// What a download found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fetched {
    /// The file was downloaded.
    Saved,
    /// Nothing is there: an HTTP 404, or a `file://` path that is missing.
    NotFound,
}

/// The network: downloads files. Only adapters are ever fetched
/// (core.md § Outside tools).
pub trait Network {
    /// Downloads `url` into the file `to`, replacing it.
    ///
    /// # Errors
    ///
    /// When the downloader cannot be run (`CouldNotRun`) or the download
    /// fails (`Failed`).
    fn download(&self, url: &str, to: &Path) -> Result<Fetched, ToolError>;

    /// The downloader's version, as it says it.
    ///
    /// # Errors
    ///
    /// When the downloader is not on the `PATH` or does not run.
    fn version(&self) -> Result<String, ToolError>;
}

/// The agent tool interface: the programs that adapt an agent, such as
/// Claude Code, to tinyfleet, each run once per request (see [`agent`]).
/// Task trackers and forges get interfaces of their own, one per kind of
/// adapter, as core.md's tool interfaces table draws them.
pub trait AgentAdapters {
    /// Asks the adapter at `adapter` what to run to start an agent.
    ///
    /// # Errors
    ///
    /// When the adapter fails.
    fn launch(
        &self,
        adapter: &Path,
        request: &agent::Launch,
    ) -> Result<agent::Launched, agent::AdapterFault>;

    /// Asks the adapter at `adapter` what state each session's agent is in.
    ///
    /// # Errors
    ///
    /// When the adapter fails.
    fn read(
        &self,
        adapter: &Path,
        request: &agent::Read,
    ) -> Result<agent::Readings, agent::AdapterFault>;

    /// Asks the adapter at `adapter` which protocol it speaks.
    ///
    /// # Errors
    ///
    /// When the adapter fails.
    fn protocol(&self, adapter: &Path) -> Result<agent::Speaks, agent::AdapterFault>;
}
