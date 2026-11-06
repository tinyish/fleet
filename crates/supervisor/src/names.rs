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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, PoisonError};

    use fleet_core::messages::{Recipient, Sender};

    use crate::testing::{FakeMux, put, spawned_row, with_seat};

    fn lock(state: &Mutex<State>) -> std::sync::MutexGuard<'_, State> {
        state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn record(state: &Mutex<State>, step: &Step) {
        assert!(lock(state).write(|w| w.record(step)).is_ok());
    }

    fn launched(session: &str) -> Step {
        Step::SessionLaunched {
            session: session.into(),
            adapter: "/a".into(),
            program: "/a".into(),
            directory: "/c".into(),
            model: None,
            prompt: None,
            fleet: None,
            seat: None,
            role: None,
        }
    }

    /// What `name` points at from `from`, for a verb taking every kind, and
    /// the fleet searched.
    fn found(state: &Mutex<State>, from: &Path, name: &str) -> (Option<PathBuf>, Option<Named>) {
        let resolved = resolve(&lock(state), from, name, Takes::SeatSessionOrMessage);
        assert!(resolved.is_ok());
        resolved.map_or((None, None), |r| (r.here, r.named))
    }

    #[test]
    fn a_seat_is_found_from_anywhere_in_its_fleet() {
        let (_root, state, fleet) = with_seat("names-seat");
        let src = fleet.directory.join("src");
        let _ = std::fs::create_dir_all(&src);
        let seat = Named::Seat(SeatKey {
            fleet: fleet.directory.clone(),
            seat: "scribe".into(),
        });
        assert_eq!(
            found(&state, &src, "scribe"),
            (Some(fleet.directory.clone()), Some(seat))
        );
    }

    #[test]
    fn a_seat_wins_over_a_session_of_the_same_id() {
        let (_root, state, fleet) = with_seat("names-shadow");
        put(
            &state,
            &FakeMux::default(),
            &spawned_row("scribe", &fleet.directory, 1),
        );
        assert!(matches!(
            found(&state, &fleet.directory, "scribe").1,
            Some(Named::Seat(_))
        ));
    }

    #[test]
    fn a_session_is_found_by_its_row_then_by_the_record() {
        let (root, state, fleet) = with_seat("names-session");
        put(
            &state,
            &FakeMux::default(),
            &spawned_row("s1", &fleet.directory, 1),
        );
        assert!(matches!(
            found(&state, &root, "s1"),
            (None, Some(Named::Session(row))) if row.id == "s1"
        ));
        record(&state, &launched("s2"));
        assert_eq!(
            found(&state, &root, "s2").1,
            Some(Named::Ended("s2".into()))
        );
    }

    #[test]
    fn a_message_is_found_in_the_record_only_by_a_verb_that_takes_one() {
        let (root, state, _) = with_seat("names-message");
        record(
            &state,
            &Step::MessageQueued {
                message: "m1".into(),
                to: Recipient::Session {
                    session: "s1".into(),
                },
                sender: Sender::Person,
                text: "hi".into(),
                fleet: None,
            },
        );
        assert_eq!(
            found(&state, &root, "m1").1,
            Some(Named::Message("m1".into()))
        );
        let resolved = resolve(&lock(&state), &root, "m1", Takes::SeatOrSession);
        assert!(resolved.as_ref().is_ok_and(|r| r.named.is_none()));
        assert_eq!(
            resolved.map(Resolved::unknown).ok(),
            Some(status::Naming::Unknown {
                name: "m1".into(),
                fleet: None,
                takes: Takes::SeatOrSession,
            })
        );
    }

    #[test]
    fn nothing_by_that_name_is_refused_with_the_fleet_searched_and_what_the_verb_takes() {
        let (root, state, fleet) = with_seat("names-unknown");
        for (from, here) in [
            (&fleet.directory, Some(fleet.directory.clone())),
            (&root, None),
        ] {
            let resolved = resolve(&lock(&state), from, "critic", Takes::SeatOrSession);
            assert!(resolved.as_ref().is_ok_and(|r| r.named.is_none()));
            assert_eq!(
                resolved.map(Resolved::unknown).ok(),
                Some(status::Naming::Unknown {
                    name: "critic".into(),
                    fleet: here,
                    takes: Takes::SeatOrSession,
                })
            );
        }
    }

    #[test]
    fn here_is_the_fleet_holding_the_canonical_directory() {
        let (root, state, fleet) = with_seat("names-here");
        let dotted = fleet.directory.join("src").join("..");
        let held = |p: &Path| here(&lock(&state), p).ok().flatten().map(|f| f.directory);
        assert_eq!(held(&dotted), Some(fleet.directory.clone()));
        assert_eq!(held(&root), None);
    }
}
