//! Why the supervisor says no to a name (core.md § Terms, name): it points
//! at nothing the command takes.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Why a name was refused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(NamingKind), derive(strum::EnumIter))
)]
#[serde(rename_all = "snake_case")]
pub enum Naming {
    /// Nothing of the kinds the command takes goes by this name.
    Unknown {
        /// The name given.
        name: String,
        /// The fleet whose seats were searched: the one holding the
        /// directory asked from, if any.
        fleet: Option<PathBuf>,
        /// The kinds the command takes.
        takes: Takes,
    },
}

/// The kinds of thing a command takes by name, in the order a name is
/// looked up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
#[serde(rename_all = "snake_case")]
pub enum Takes {
    /// A seat of the fleet here, else a session: message and end.
    SeatOrSession,
    /// A seat of the fleet here, else a session, else a message: wait.
    SeatSessionOrMessage,
}

impl From<Naming> for super::Status {
    fn from(refusal: Naming) -> Self {
        super::Status::Naming(refusal)
    }
}
