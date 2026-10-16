//! How a command ends, as data: `Result<Response, Status>`. The CLI's own
//! `Response` and `Status` are what it tells the person; the supervisor's
//! are always written `protocol::Response` and `protocol::Status`. words.rs
//! turns these into sentences, and output.rs prints them.

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use fleet_core::paths::PathError;
use fleet_core::tools::ToolError;
use protocol::ProtocolError;

/// What a command did.
#[derive(Debug)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(ResponseKind), derive(strum::EnumIter))
)]
pub(crate) enum Response {
    /// `fleet create` did this.
    Created {
        /// What the supervisor said create did.
        response: protocol::response::Create,
        /// Whether create found the supervisor running or started it.
        startup: Startup,
    },
    /// `fleet start` did this.
    Started {
        /// Whether start found the supervisor running or started it.
        startup: Startup,
    },
    /// Every fleet is off and the supervisor is stopped.
    SupervisorStopped,
    /// Stop was asked for and the supervisor was not running.
    SupervisorWasNotRunning,
}

/// How a command found the supervisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
pub(crate) enum Startup {
    /// It was already running.
    WasRunning,
    /// This command started it.
    Started,
}

/// Why a command did not do what was asked, and the exit code that says so.
#[derive(Debug)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(StatusKind), derive(strum::EnumIter))
)]
pub(crate) enum Status {
    /// The supervisor said no.
    Refused(protocol::Status),
    /// The CLI could not go on.
    Cannot(Problem),
    /// The command needs the supervisor, it is not running, and nobody may
    /// start it (no terminal to ask, no `--non-interactive`).
    NotRunning,
    /// Something may or may not have happened.
    CouldNotTell(Doubt),
}

impl Status {
    /// The exit code a script reads: 1 refused or cannot, 3 not running,
    /// 4 could not tell. (A command that finishes exits 0.)
    pub(crate) fn exit_code(&self) -> u8 {
        match self {
            Status::Refused(_) | Status::Cannot(_) => 1,
            Status::NotRunning => 3,
            Status::CouldNotTell(_) => 4,
        }
    }
}

/// Why the CLI could not go on, before or around asking the supervisor.
#[derive(Debug)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(ProblemKind), derive(strum::EnumIter))
)]
pub(crate) enum Problem {
    /// This is not macOS or Linux.
    UnsupportedOs,
    /// The state directory could not be worked out.
    StateDir(PathError),
    /// The socket path would be too long for a Unix socket.
    SocketPathTooLong(PathBuf),
    /// The current directory could not be read.
    NoCurrentDir(io::Error),
    /// A path given on the command line could not be used.
    BadPath {
        /// The path.
        path: PathBuf,
        /// What the system said.
        error: io::Error,
    },
    /// This binary's own path could not be found.
    NoBinary(io::Error),
    /// The service manager would not start or stop the supervisor.
    ServiceManager {
        /// What was asked of it.
        doing: Doing,
        /// What it said.
        error: ToolError,
    },
}

/// What was asked of the service manager.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
pub(crate) enum Doing {
    /// Start the supervisor.
    Start,
    /// Stop the supervisor.
    Stop,
}

/// Why the CLI cannot tell whether something happened.
#[derive(Debug)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(DoubtKind), derive(strum::EnumIter))
)]
pub(crate) enum Doubt {
    /// The supervisor was started but did not answer in time.
    NoAnswer {
        /// How long the CLI waited.
        waited: Duration,
        /// Where the supervisor's log can be read.
        log: String,
    },
    /// The supervisor was stopped but still answers.
    StillAnswers {
        /// How long the CLI waited.
        waited: Duration,
    },
    /// Talking to the supervisor failed part way.
    Socket {
        /// What the CLI was asking for.
        during: During,
        /// What went wrong.
        error: ProtocolError,
    },
    /// The supervisor answered something the request does not expect.
    OddReply {
        /// The request it was answering.
        to: &'static str,
    },
}

/// What the CLI was asking the supervisor for when talking failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(DuringKind), derive(strum::EnumIter))
)]
pub(crate) enum During {
    /// Creating a fleet.
    Creating,
    /// Turning every fleet on, after finding or starting the supervisor.
    TurningOn(Startup),
    /// Turning every fleet off.
    TurningOff,
}
