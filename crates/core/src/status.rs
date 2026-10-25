//! Why the supervisor says no.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::agent::{AdapterFault, Verb};

/// Why the supervisor says no: a rule's verdict, or a problem with a path or
/// a file. Facts only: the CLI words them (core.md invariant 9). The CLI and
/// the protocol write it as `protocol::Status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(StatusKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Status {
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
    /// A `fleet.toml` no fleet owns is already there, listing other
    /// projects than the ones asked for; it was left alone.
    ConfigDiffers {
        /// The file.
        path: PathBuf,
        /// The projects it lists.
        listed: Vec<PathBuf>,
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
    /// Two fleets' configs both name this project.
    AlsoClaimed {
        /// The project.
        project: PathBuf,
        /// The other fleet that names it.
        fleet: PathBuf,
    },
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
    /// Remove was run inside a fleet's directory or one of its projects, not
    /// at the fleet's directory.
    NotFleetDirectory {
        /// The directory remove was run in.
        directory: PathBuf,
        /// The fleet it is inside.
        fleet: PathBuf,
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
    /// The request could not be understood.
    Malformed {
        /// What was wrong with it.
        error: String,
    },
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
    /// No session has this id.
    NoSuchSession {
        /// The id asked for.
        session: String,
    },
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

impl Status {
    /// Why a path could not be used, from what the system said: it does not
    /// exist, or it could not be read.
    #[must_use]
    pub fn unusable(path: &Path, e: &io::Error) -> Status {
        let path = path.to_path_buf();
        if e.kind() == io::ErrorKind::NotFound {
            Status::Missing { path }
        } else {
            Status::Unreadable {
                path,
                error: e.to_string(),
            }
        }
    }
}
