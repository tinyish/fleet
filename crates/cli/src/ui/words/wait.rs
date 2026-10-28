//! The words for waiting: what a wait came to, and what goes wrong.

use std::time::Duration;

use fleet_core::agent::{SessionState, StateName};
use fleet_core::messages::Dropped;
use fleet_core::protocol::Waited;
use fleet_core::status::Wait;

use crate::ui::output::Ink;

/// What `fleet wait` waited on came to one of its states.
pub(super) fn reached(on: &Waited, session: &str, state: SessionState, ink: Ink) -> String {
    format!("{} {}", named(on, Some(session), ink), is(state))
}

/// What `fleet wait` says once it waits, on stderr: the time, and what it
/// waits for.
pub(crate) fn waiting(
    on: &Waited,
    session: Option<&str>,
    states: &[StateName],
    ink: Ink,
) -> String {
    let to_be = states
        .iter()
        .map(|s| match s {
            StateName::Starting => "starting",
            StateName::Working => "working",
            StateName::Idle => "idle",
            StateName::NeedsYou => "needing you",
            StateName::Gone => "gone",
            StateName::Unknown => "unknown",
        })
        .collect::<Vec<_>>()
        .join(" or ");
    let what = match (on, states.is_empty()) {
        (Waited::Message { message }, true) => {
            format!("{} to be delivered", ink.accent(message))
        }
        (Waited::Message { message }, false) => format!(
            "{} to be delivered and its agent to be {to_be}",
            ink.accent(message)
        ),
        (_, _) => format!("{} to be {to_be}", named(on, session, ink)),
    };
    format!("{} waiting for {what}", ink.muted(super::events::clock()))
}

/// The message waited on was typed in.
pub(super) fn delivered(message: &str, session: &str, ink: Ink) -> String {
    format!(
        "{} was delivered to {}",
        ink.accent(message),
        ink.accent(session)
    )
}

/// `fleet wait` ran out of time, with where it still stands.
pub(super) fn timed_out(
    on: &Waited,
    session: Option<&str>,
    state: Option<SessionState>,
    after: Duration,
    ink: Ink,
) -> String {
    let still = match (on, state) {
        (Waited::Message { .. }, _) => "is still waiting to be typed in".into(),
        (_, Some(state)) => format!("is still {}", super::overview::state_word(state)),
        (_, None) => "has no state yet".into(),
    };
    format!("{} {still} after {}", named(on, session, ink), took(after))
}

/// What is waited on: a seat as `scribe (s1…)`, a session or a message by
/// its id.
fn named(on: &Waited, session: Option<&str>, ink: Ink) -> String {
    match (on, session) {
        (Waited::Seat { seat, .. }, Some(session)) => {
            format!("{} ({})", ink.accent(seat), ink.accent(session))
        }
        (Waited::Seat { seat, .. }, None) => ink.accent(seat),
        (Waited::Session { session }, _) => ink.accent(session),
        (Waited::Message { message }, _) => ink.accent(message),
    }
}

/// A state after a name: `is idle`, or `needs you: …`.
fn is(state: SessionState) -> String {
    let word = super::overview::state_word(state);
    match state {
        SessionState::NeedsYou { .. } => word,
        _ => format!("is {word}"),
    }
}

/// A duration as a person writes it: `5m`, `1m 30s`.
fn took(after: Duration) -> String {
    jiff::SignedDuration::try_from(after)
        .map_or_else(|_| format!("{}s", after.as_secs()), |d| format!("{d:#}"))
}

/// Why the supervisor refused a wait, or why its state can never come.
pub(super) fn refused(refusal: &Wait, ink: Ink) -> String {
    match refusal {
        Wait::NoSuchName {
            name,
            fleet: Some(fleet),
        } => format!(
            "the fleet in {} has no seat {}, and no session or message has that id",
            ink.path(fleet),
            ink.accent(name)
        ),
        Wait::NoSuchName { name, fleet: None } => {
            format!("no session or message has the id {}", ink.accent(name))
        }
        Wait::NoStates { name } => {
            format!("say which states to wait for {} to reach", ink.accent(name))
        }
        Wait::Gone { session } => format!("the session {} is gone", ink.accent(session)),
        Wait::Ended { session } => format!("the session {} was ended", ink.accent(session)),
        Wait::SeatRemoved { fleet, seat } => format!(
            "the seat {} was taken out of the fleet in {}",
            ink.accent(seat),
            ink.path(fleet)
        ),
        Wait::FleetRemoved { fleet } => {
            format!("the fleet in {} was removed", ink.path(fleet))
        }
        Wait::Dropped { message, why } => {
            format!("{} was dropped: {}", ink.accent(message), dropped(*why))
        }
    }
}

/// What to do about it.
pub(super) fn hint(refusal: &Wait, ink: Ink) -> Option<String> {
    match refusal {
        Wait::NoSuchName { .. } => Some(format!(
            "run {} to see the seats and sessions there are",
            ink.accent("fleet status")
        )),
        Wait::NoStates { name } => Some(format!(
            "for example: {}",
            ink.accent(format!("fleet wait {name} idle"))
        )),
        Wait::Gone { .. }
        | Wait::Ended { .. }
        | Wait::SeatRemoved { .. }
        | Wait::FleetRemoved { .. }
        | Wait::Dropped { .. } => None,
    }
}

/// Why a message was dropped, after "dropped:".
pub(super) fn dropped(why: Dropped) -> &'static str {
    match why {
        Dropped::SessionEnded => "its session ended",
        Dropped::SeatRemoved => "its seat was taken out",
        Dropped::FleetRemoved => "its fleet was removed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::ERR;
    use std::path::PathBuf;

    fn plain(text: &str) -> String {
        console::strip_ansi_codes(text).into_owned()
    }

    #[test]
    fn nobody_by_that_name_says_where_it_looked() {
        let here = Wait::NoSuchName {
            name: "critic".into(),
            fleet: Some(PathBuf::from("/code/app")),
        };
        assert_eq!(
            plain(&refused(&here, ERR.piped())),
            "the fleet in /code/app has no seat critic, and no session or message has that id"
        );
    }

    #[test]
    fn a_dropped_message_says_why() {
        let dropped = Wait::Dropped {
            message: "m1".into(),
            why: Dropped::SeatRemoved,
        };
        assert_eq!(
            plain(&refused(&dropped, ERR.piped())),
            "m1 was dropped: its seat was taken out"
        );
    }

    #[test]
    fn the_endings_of_a_wait_read_plainly() {
        use crate::ui::output::OUT;
        use fleet_core::agent::SessionState;
        use fleet_core::protocol::Waited;
        use std::time::Duration;
        let seat = Waited::Seat {
            fleet: "/code/app".into(),
            seat: "scribe".into(),
        };
        assert_eq!(
            plain(&reached(
                &seat,
                "s1a2b3c4d5",
                SessionState::Idle,
                OUT.piped()
            )),
            "scribe (s1a2b3c4d5) is idle"
        );
        assert_eq!(
            plain(&delivered("m9f8e7d6c5", "s1a2b3c4d5", OUT.piped())),
            "m9f8e7d6c5 was delivered to s1a2b3c4d5"
        );
        assert_eq!(
            plain(&timed_out(
                &Waited::Session {
                    session: "s1".into()
                },
                Some("s1"),
                Some(SessionState::Working),
                Duration::from_mins(5),
                ERR.piped()
            )),
            "s1 is still working after 5m"
        );
        assert_eq!(
            plain(&timed_out(
                &Waited::Message {
                    message: "m1".into()
                },
                None,
                None,
                Duration::from_secs(90),
                ERR.piped()
            )),
            "m1 is still waiting to be typed in after 1m 30s"
        );
    }

    #[test]
    fn wait_says_what_it_waits_for_as_it_starts() {
        use fleet_core::agent::StateName;
        use fleet_core::protocol::Waited;
        let after_time = |line: String| {
            plain(&line)
                .split_once(' ')
                .map(|(_, rest)| rest.to_owned())
                .unwrap_or_default()
        };
        let seat = Waited::Seat {
            fleet: "/code/app".into(),
            seat: "scribe".into(),
        };
        let m1 = Waited::Message {
            message: "m1".into(),
        };
        let ink = ERR.piped();
        assert_eq!(
            after_time(waiting(&seat, Some("s1"), &[StateName::Idle], ink)),
            "waiting for scribe (s1) to be idle"
        );
        assert_eq!(
            after_time(waiting(
                &seat,
                None,
                &[StateName::Idle, StateName::NeedsYou],
                ink
            )),
            "waiting for scribe to be idle or needing you"
        );
        assert_eq!(
            after_time(waiting(&m1, None, &[], ink)),
            "waiting for m1 to be delivered"
        );
        assert_eq!(
            after_time(waiting(&m1, None, &[StateName::Idle], ink)),
            "waiting for m1 to be delivered and its agent to be idle"
        );
    }
}
