//! Why the supervisor says no.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

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
    /// A `fleet.toml` is already there and no fleet owns it; it was left alone.
    StrayConfig {
        /// The file.
        path: PathBuf,
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
