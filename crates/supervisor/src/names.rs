//! What a name points at (core.md § Terms, name), from state alone: the
//! fleet holding the directory a command was given in, and a seat of it, a
//! session, or a message. Each verb takes the kinds it acts on, and keeps
//! its own rule for whether what it found is live enough.

use std::path::{Path, PathBuf};

use fleet_core::fleets::{self, Fleet};
use fleet_core::record::Step;
use fleet_core::seats::SeatKey;
use fleet_core::status::{self, Takes};
use state::{SessionRow, State, StateError};

/// What a name points at.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Named {
    /// A seat of the fleet here.
    Seat(SeatKey),
    /// A session with a row: running, or its pane ended and not yet
    /// cleared.
    Session(SessionRow),
    /// A session only in the record: ended, or cleared as gone.
    Ended(String),
    /// A message queued in the record.
    Message(String),
}

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

/// What `name` points at from `directory`, among the kinds `takes`: a
/// seat of the fleet holding it, else a session with a row, else a session
/// in the record, else, when the verb takes one, a message in the record.
/// A kind the verb does not take is never looked up, so it is unknown.
pub(crate) fn resolve(
    state: &State,
    directory: &Path,
    name: &str,
    takes: Takes,
) -> Result<Resolved, StateError> {
    let here = here(state, directory)?.map(|f| f.directory);
    let named = named(state, here.as_ref(), name, takes)?;
    Ok(Resolved {
        name: name.to_owned(),
        takes,
        here,
        named,
    })
}

fn named(
    state: &State,
    here: Option<&PathBuf>,
    name: &str,
    takes: Takes,
) -> Result<Option<Named>, StateError> {
    if let Some(fleet) = here
        && state.roster(fleet)?.seats.iter().any(|s| s.name == name)
    {
        return Ok(Some(Named::Seat(SeatKey {
            fleet: fleet.clone(),
            seat: name.to_owned(),
        })));
    }
    if let Some(row) = state.sessions()?.into_iter().find(|r| r.id == name) {
        return Ok(Some(Named::Session(row)));
    }
    let launched = state
        .steps_with_session(name)?
        .iter()
        .any(|e| matches!(e.step, Step::SessionLaunched { .. }));
    if launched {
        return Ok(Some(Named::Ended(name.to_owned())));
    }
    if takes == Takes::SeatOrSession {
        return Ok(None);
    }
    let queued = state
        .steps_with_message(name)?
        .iter()
        .any(|e| matches!(e.step, Step::MessageQueued { .. }));
    Ok(queued.then(|| Named::Message(name.to_owned())))
}

/// Whether `row` is a session of seat `seat` of the fleet in `fleet`.
pub(crate) fn of_seat(row: &SessionRow, fleet: &Path, seat: &str) -> bool {
    row.fleet.as_deref() == Some(fleet) && row.seat.as_deref() == Some(seat)
}
