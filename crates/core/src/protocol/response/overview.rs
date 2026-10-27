//! What `fleet status` shows, as the supervisor answers it: re-exported
//! from `protocol::response`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::agent::{AdapterFault, SessionState};
use crate::status::Status;

/// One session and its state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listed {
    /// Its id.
    pub session: String,
    /// The adapter program that launched it.
    pub adapter: PathBuf,
    /// Where its agent works.
    pub directory: PathBuf,
    /// Its state now.
    pub state: SessionState,
    /// When it was launched.
    pub launch_time_millis: i64,
    /// How many messages wait for this session.
    #[serde(default, skip_serializing_if = "is_default")]
    pub waiting: usize,
}

/// An adapter that failed to read its sessions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fault {
    /// The adapter program.
    pub adapter: PathBuf,
    /// How it failed.
    pub fault: AdapterFault,
}

/// What `fleet status` shows: the fleet holding the directory asked
/// about, or, when none holds it, every fleet and the sessions of none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Overview {
    /// The fleets, by directory.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fleets: Vec<FleetOverview>,
    /// Sessions of a fleet since removed. Only when every fleet is
    /// shown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loose: Vec<Listed>,
    /// The adapters that failed to read their sessions, which are
    /// unknown.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub faults: Vec<Fault>,
}

/// One fleet's seats.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetOverview {
    /// The fleet's directory.
    pub directory: PathBuf,
    /// Whether any of its projects is on, so its seats are kept filled.
    pub on: bool,
    /// Its seats, by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seats: Vec<SeatOverview>,
    /// Sessions of seats its config no longer has.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orphans: Vec<Orphan>,
    /// Sessions spawned from its roles, oldest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spawned: Vec<Spawned>,
}

/// One seat, and its session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatOverview {
    /// The seat's name.
    pub name: String,
    /// The label of its role.
    pub role: String,
    /// Its session: the live one, else the newest; none before its
    /// first launch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<Listed>,
    /// Whether that session was launched from its role as it was
    /// before an edit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub older_role: bool,
    /// Whether it is backing off: no session alive, and a wait.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub down: Option<Down>,
    /// How many messages wait for this seat, whichever session takes them.
    #[serde(default, skip_serializing_if = "is_default")]
    pub waiting: usize,
}

/// A seat backing off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Down {
    /// Why its last launch failed, or why nothing could be launched;
    /// none when its sessions keep ending.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub why: Option<Status>,
    /// When it is tried next, by the supervisor's clock.
    pub next_try_millis: i64,
}

/// A session spawned from a role of this fleet: never revived, and shown
/// gone once its agent ends, until it is ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spawned {
    /// The label of the role it was spawned from.
    pub role: String,
    /// The session.
    pub session: Listed,
}

/// A session of a seat its fleet's config no longer has: never revived.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orphan {
    /// The seat it was launched for.
    pub seat: String,
    /// The session.
    pub session: Listed,
}

/// Whether `value` is its type's default, so the wire leaves it out.
fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}
