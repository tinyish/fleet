//! What a name points at (core.md § Terms, name), from where it was
//! given: the fleet holding the directory a command was given in, whose
//! seats state searches first. Each verb takes the kinds it acts on, and
//! reads a session's life from its row.

use std::path::{Path, PathBuf};

use fleet_core::fleets::{self, Fleet};
use fleet_core::status::{self, Takes};
use state::{Named, SessionRow, State, StateError};

/// A name, and what it points at from where it was given, among the kinds
/// the verb takes.
pub(crate) struct Resolved {
    /// The name given.
    pub(crate) name: String,
    /// The kinds the verb takes.
    pub(crate) takes: Takes,
    /// The fleet holding the directory it was given in, whose seats were
    /// searched.
    pub(crate) here: Option<PathBuf>,
    /// What it points at, if anything of the kinds the verb takes.
    pub(crate) named: Option<Named>,
}

impl Resolved {
    /// The refusal for a name that points at nothing the verb takes.
    pub(crate) fn unknown(self) -> status::Naming {
        status::Naming::Unknown {
            name: self.name,
            fleet: self.here,
            takes: self.takes,
        }
    }
}

/// The fleet holding `directory`, made canonical.
pub(crate) fn here(state: &State, directory: &Path) -> Result<Option<Fleet>, StateError> {
    let directory = crate::canonical(directory);
    Ok(fleets::holding(&state.fleets()?, &directory).cloned())
}

/// What `name` points at from `directory`, among the kinds `takes`, as
/// [`State::named`] finds it from the fleet holding `directory`.
pub(crate) fn resolve(
    state: &State,
    directory: &Path,
    name: &str,
    takes: Takes,
) -> Result<Resolved, StateError> {
    let here = here(state, directory)?.map(|f| f.directory);
    let named = state.named(here.as_deref(), name, takes)?;
    Ok(Resolved {
        name: name.to_owned(),
        takes,
        here,
        named,
    })
}

/// Whether `row` is a session of seat `seat` of the fleet in `fleet`.
pub(crate) fn of_seat(row: &SessionRow, fleet: &Path, seat: &str) -> bool {
    row.fleet.as_deref() == Some(fleet) && row.seat.as_deref() == Some(seat)
}
