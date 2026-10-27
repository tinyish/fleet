//! Why the supervisor says no.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::{AdapterFault, Verb};

/// Why the supervisor says no, grouped by what it is about: where a fleet
/// can go, one project's on or off, a path or a config file, agents'
/// sessions, or naming and fetching adapters. Facts only: the CLI words them
/// (core.md invariant 9), one words file and one specimen file per area.
/// The CLI and the protocol write it as `protocol::Status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(StatusKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Where a fleet can go: create's and remove's rules.
    Fleet(Fleet),
    /// One project, turned on or off.
    Project(Project),
    /// A path, or a fleet's config file.
    File(File),
    /// Agents' sessions: their adapters' answers, and tmux.
    Session(Session),
    /// Naming and fetching adapters.
    Adapter(Adapter),
    /// Messages: who one is for, and its text.
    Message(Message),
    /// The request could not be understood.
    Malformed {
        /// What was wrong with it.
        error: String,
    },
}

/// Where a fleet can go: the rules create and remove apply.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(FleetKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Fleet {
    /// A default fleet's directory is in no git repository.
    DirectoryNotInRepo {
        /// The directory.
        path: PathBuf,
    },
    /// A default fleet's directory is inside a repository, not at its root.
    DirectoryNotRepoRoot {
        /// The directory.
        path: PathBuf,
        /// The repository's root.
        root: PathBuf,
    },
    /// A project is in no git repository.
    ProjectNotInRepo {
        /// The project path given.
        path: PathBuf,
    },
    /// A project is inside a repository, not at its root.
    ProjectNotRepoRoot {
        /// The project path given.
        path: PathBuf,
        /// The repository's root.
        root: PathBuf,
    },
    /// A project given with `--project` is the fleet directory itself.
    ProjectIsFleetDirectory {
        /// The directory.
        path: PathBuf,
    },
    /// A wingman fleet's directory is inside one of its own projects.
    WingmanInsideProject {
        /// The fleet directory.
        directory: PathBuf,
        /// The project it is inside.
        project: PathBuf,
    },
    /// A fleet is already in the directory, serving other projects.
    AlreadyHereServingOthers {
        /// The fleet directory.
        directory: PathBuf,
        /// What the fleet there serves.
        serving: Vec<PathBuf>,
        /// What was asked for.
        asked: Vec<PathBuf>,
    },
    /// The directory is inside another fleet's directory.
    InsideFleet {
        /// The directory asked for.
        directory: PathBuf,
        /// The other fleet's directory.
        fleet: PathBuf,
    },
    /// The directory contains another fleet's directory.
    ContainsFleet {
        /// The directory asked for.
        directory: PathBuf,
        /// The other fleet's directory.
        fleet: PathBuf,
    },
    /// The directory is inside a project another fleet serves.
    InsideServedProject {
        /// The directory asked for.
        directory: PathBuf,
        /// The project it is inside.
        project: PathBuf,
        /// The fleet serving that project.
        fleet: PathBuf,
    },
    /// A project is already served by another fleet.
    AlreadyServed {
        /// The project.
        project: PathBuf,
        /// The fleet serving it.
        fleet: PathBuf,
    },
    /// Two fleets' configs both name this project.
    AlsoClaimed {
        /// The project.
        project: PathBuf,
        /// The other fleet that names it.
        fleet: PathBuf,
    },
    /// Remove was run inside a fleet's directory or one of its projects, not
    /// at the fleet's directory.
    NotFleetDirectory {
        /// The directory remove was run in.
        directory: PathBuf,
        /// The fleet it is inside.
        fleet: PathBuf,
    },
}

/// One project, turned on or off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(ProjectKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Project {
    /// No fleet serves this path.
    NotServed {
        /// The path asked about.
        path: PathBuf,
    },
    /// This path is inside a project, not at its root.
    InsideProject {
        /// The path asked about.
        path: PathBuf,
        /// The project it is inside.
        project: PathBuf,
    },
    /// Re-reading its fleet's config just dropped this project.
    NoLongerServed {
        /// The project.
        project: PathBuf,
        /// The fleet whose config no longer names it.
        fleet: PathBuf,
    },
    /// This project could not be turned on: its fleet's config did not read,
    /// or clashed with another fleet's. Its own on or off, and its fleet's
    /// other projects', stay as they were.
    CannotTurnOn {
        /// The project.
        project: PathBuf,
        /// Its fleet.
        fleet: PathBuf,
        /// Why the fleet's config cannot be used.
        why: Box<Status>,
    },
}

/// A path, or a fleet's config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(FileKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum File {
    /// A path does not exist.
    Missing {
        /// The path.
        path: PathBuf,
    },
    /// A path could not be read.
    Unreadable {
        /// The path.
        path: PathBuf,
        /// What the system said.
        error: String,
    },
    /// A file could not be written; nothing was registered.
    CannotWrite {
        /// The file.
        path: PathBuf,
        /// What the system said.
        error: String,
    },
    /// A fleet's config is not valid TOML, or holds a key this version does
    /// not know.
    BadConfig {
        /// The config file.
        path: PathBuf,
        /// The line, counting from 1, when the parser knows.
        line: Option<usize>,
        /// The column on that line, counting from 1, when the parser knows.
        column: Option<usize>,
        /// What the parser said, in one line.
        error: String,
    },
    /// A `fleet.toml` no fleet owns is already there, listing other
    /// projects than the ones asked for; it was left alone.
    ConfigDiffers {
        /// The file.
        path: PathBuf,
        /// The projects it lists.
        listed: Vec<PathBuf>,
    },
    /// A role's label or a seat's name in a fleet's config is not letters,
    /// digits, `-` and `_`, starting with a letter or a digit.
    BadName {
        /// The config file.
        path: PathBuf,
        /// The name.
        name: String,
    },
    /// A seat in a fleet's config names a role the config does not have.
    UnknownRole {
        /// The config file.
        path: PathBuf,
        /// The seat.
        seat: String,
        /// The role it names.
        role: String,
    },
    /// A role's adapter in a fleet's config is not one tinyfleet can name.
    RoleAdapter {
        /// The config file.
        path: PathBuf,
        /// The role.
        role: String,
        /// Why: an adapter refusal.
        why: Box<Status>,
    },
}

/// Agents' sessions: their adapters' answers, and tmux.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(SessionKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Session {
    /// An agent adapter failed to answer.
    AdapterFailed {
        /// The adapter program.
        adapter: PathBuf,
        /// What it was asked.
        verb: Verb,
        /// How it failed.
        fault: AdapterFault,
    },
    /// tmux could not be run: not installed, or not runnable.
    MultiplexerCouldNotRun {
        /// What the system said.
        error: String,
    },
    /// tmux failed.
    MultiplexerFailed {
        /// The command that failed.
        command: String,
        /// What it said.
        said: String,
    },
    /// No fleet holds the directory a spawn was asked from.
    NoFleetHere {
        /// The directory.
        directory: PathBuf,
    },
    /// The fleet has no role with this label.
    NoSuchRole {
        /// The label asked for.
        role: String,
        /// The fleet's directory.
        fleet: PathBuf,
    },
    /// The directory named for a spawned agent is not one of the fleet's
    /// projects by its root.
    NotAProject {
        /// The directory named.
        path: PathBuf,
        /// The fleet's directory.
        fleet: PathBuf,
    },
    /// The seat has no live session to end.
    SeatNotRunning {
        /// The seat.
        seat: String,
        /// The fleet's directory.
        fleet: PathBuf,
    },
    /// No seat of `fleet`, the fleet holding the directory if any, and no
    /// session has this name.
    NoSuchSeatOrSession {
        /// The name asked for.
        name: String,
        /// The fleet searched, if any.
        fleet: Option<PathBuf>,
    },
    /// Launching the agent would make a tmux command longer than it takes:
    /// its prompt is too long.
    LaunchTooLong {
        /// How long the command would be, in bytes.
        bytes: usize,
        /// The most tmux takes, in bytes.
        limit: usize,
    },
}

/// Naming and fetching adapters.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(AdapterKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Adapter {
    /// An adapter was named without its version.
    AdapterUnpinned {
        /// The name.
        name: String,
        /// Its latest release's version, when it could be looked up.
        latest: Option<String>,
    },
    /// Not an adapter: a path names one by a `/`, a release as
    /// `<name>@<version>`.
    NotAnAdapter {
        /// What was given.
        reference: String,
    },
    /// fleet-adapters has no such release.
    NoSuchRelease {
        /// The adapter's name.
        name: String,
        /// The version asked for.
        version: String,
    },
    /// A release's manifest does not read, or is another release's.
    BadManifest {
        /// Where it was downloaded from.
        url: String,
        /// What was wrong with it.
        error: String,
    },
    /// A release has no build for this machine.
    NoBuildFor {
        /// The adapter's name.
        name: String,
        /// Its version.
        version: String,
        /// This machine's target.
        target: String,
    },
    /// A downloaded adapter's SHA-256 is not its manifest's.
    ChecksumMismatch {
        /// The adapter's name.
        name: String,
        /// Its version.
        version: String,
        /// The manifest's.
        expected: String,
        /// The download's.
        got: String,
    },
    /// An adapter speaks a protocol this fleet does not.
    ProtocolMismatch {
        /// The adapter, as it was named.
        adapter: String,
        /// The protocol it speaks.
        speaks: u32,
        /// The protocols this fleet speaks.
        fleet_speaks: Vec<u32>,
    },
    /// A download failed.
    DownloadFailed {
        /// What was being downloaded.
        url: String,
        /// What the downloader said.
        said: String,
    },
    /// The downloader, curl, could not be run.
    NoDownloader {
        /// What the system said.
        error: String,
    },
    /// A release does not hold together.
    ReleaseMismatch {
        /// The file of the release it is about.
        url: String,
        /// How it does not.
        what: Mismatch,
    },
    /// A fetched adapter could not be kept in the state directory.
    AdapterNotKept {
        /// The file or directory that could not be written.
        path: PathBuf,
        /// What the system said.
        error: String,
    },
}

/// How a release does not hold together.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mismatch {
    /// Its manifest is another release's.
    OtherRelease {
        /// The adapter it names.
        name: String,
        /// The version it names.
        version: String,
    },
    /// Its build speaks another protocol than its manifest says.
    BuildSpeaks {
        /// What the manifest says.
        manifest: u32,
        /// What the build answers.
        build: u32,
    },
    /// Its manifest lists a build the release does not have.
    BuildMissing,
}

/// Messages: who one is for, and its text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(MessageKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Message {
    /// No seat of the fleet here, and no session, goes by this name.
    NoSuchRecipient {
        /// The name given.
        to: String,
        /// The fleet whose seats were searched: the one holding the
        /// directory sent from, if any.
        fleet: Option<PathBuf>,
    },
    /// The session's pane has ended.
    SessionGone {
        /// Its id.
        session: String,
    },
    /// The text is empty, or only blanks.
    EmptyText,
    /// The text is longer than a message takes.
    TooLong {
        /// How long it is, in bytes.
        bytes: usize,
        /// The most a message takes, in bytes.
        limit: usize,
    },
}

impl From<Fleet> for Status {
    fn from(refusal: Fleet) -> Self {
        Status::Fleet(refusal)
    }
}

impl From<Project> for Status {
    fn from(refusal: Project) -> Self {
        Status::Project(refusal)
    }
}

impl From<File> for Status {
    fn from(refusal: File) -> Self {
        Status::File(refusal)
    }
}

impl From<Session> for Status {
    fn from(refusal: Session) -> Self {
        Status::Session(refusal)
    }
}

impl From<Adapter> for Status {
    fn from(refusal: Adapter) -> Self {
        Status::Adapter(refusal)
    }
}

impl From<Message> for Status {
    fn from(refusal: Message) -> Self {
        Status::Message(refusal)
    }
}

impl Status {
    /// Why a path could not be used, from what the system said: it does not
    /// exist, or it could not be read.
    #[must_use]
    pub fn unusable(path: &Path, e: &io::Error) -> Status {
        let path = path.to_path_buf();
        if e.kind() == io::ErrorKind::NotFound {
            File::Missing { path }.into()
        } else {
            File::Unreadable {
                path,
                error: e.to_string(),
            }
            .into()
        }
    }
}
