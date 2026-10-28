//! Why the supervisor says no to a wait: what it names, and why the state
//! it waits for can never come.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::messages::Dropped;

/// Why a wait was refused, or ended without what it waited for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(WaitKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Wait {
    /// No seat of the fleet here, no session and no message goes by this
    /// name.
    NoSuchName {
        /// The name given.
        name: String,
        /// The fleet whose seats were searched: the one holding the
        /// directory asked from, if any.
        fleet: Option<PathBuf>,
    },
    /// A seat or a session was given no state to wait for.
    NoStates {
        /// The name given.
        name: String,
    },
    /// The session waited on by its id is gone, and gone was not asked
    /// for.
    Gone {
        /// Its id.
        session: String,
    },
    /// The session waited on by its id was ended.
    Ended {
        /// Its id.
        session: String,
    },
    /// The seat waited on was taken out of its fleet's config.
    SeatRemoved {
        /// The fleet's directory.
        fleet: PathBuf,
        /// The seat.
        seat: String,
    },
    /// The fleet of the seat waited on was removed, or forgotten.
    FleetRemoved {
        /// The fleet's directory.
        fleet: PathBuf,
    },
    /// The message waited on was dropped.
    Dropped {
        /// Its id.
        message: String,
        /// Why.
        why: Dropped,
    },
}

impl From<Wait> for super::Status {
    fn from(refusal: Wait) -> Self {
        super::Status::Wait(refusal)
    }
}
