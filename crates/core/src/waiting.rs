//! The rule of a wait (core.md § Primitives, States): what a run of
//! recorded steps means for a wait on a seat, a session or a message.
//! Pure: the supervisor reads the steps and holds the poll.

use std::path::Path;

use crate::agent::{SessionState, StateName};
use crate::protocol::{Event, Waited};
use crate::record::Step;
use crate::status;

/// What a run of steps means for a wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// A session of what was waited on reads one of the states.
    Reached {
        /// The session.
        session: String,
        /// Its state.
        state: SessionState,
    },
    /// The message was typed into `session`.
    Delivered {
        /// The session.
        session: String,
    },
    /// What was waited for can never come.
    Never(status::Wait),
    /// Nothing here decides it.
    Keep,
}

/// What `steps`, oldest first, mean for a wait on `on` for `states`: the
/// first step that decides it, or `Keep`.
#[must_use]
pub fn judge(on: &Waited, states: &[StateName], steps: &[Event]) -> Verdict {
    steps
        .iter()
        .find_map(|e| match on {
            Waited::Seat { fleet, seat } => for_seat(fleet, seat, states, &e.step),
            Waited::Session { session } => for_session(session, states, &e.step),
            Waited::Message { message } => for_message(message, &e.step),
        })
        .unwrap_or(Verdict::Keep)
}

/// A seat: any of its sessions reading a wanted state, or gone when gone
/// is wanted; never once it is taken out or its fleet goes.
fn for_seat(fleet: &Path, seat: &str, states: &[StateName], step: &Step) -> Option<Verdict> {
    let wanted = |s: SessionState| states.contains(&s.name());
    let mine = |f: &str, s: &str| Path::new(f) == fleet && s == seat;
    match step {
        Step::SessionStateChanged {
            session,
            fleet: Some(f),
            seat: Some(s),
            state,
        } if mine(f, s) && wanted(*state) => Some(reached(session, *state)),
        Step::SessionGone {
            session,
            fleet: f,
            seat: s,
        } if mine(f, s) && wanted(SessionState::Gone) => Some(reached(session, SessionState::Gone)),
        Step::SeatRemoved { fleet: f, seat: s } if mine(f, s) => {
            Some(Verdict::Never(status::Wait::SeatRemoved {
                fleet: fleet.to_path_buf(),
                seat: seat.to_owned(),
            }))
        }
        Step::FleetRemoved { directory, .. } | Step::FleetForgotten { directory }
            if Path::new(directory) == fleet =>
        {
            Some(Verdict::Never(status::Wait::FleetRemoved {
                fleet: fleet.to_path_buf(),
            }))
        }
        _ => None,
    }
}

/// One session: a wanted state; never once it is gone (unless gone is
/// wanted) or ended.
fn for_session(session: &str, states: &[StateName], step: &Step) -> Option<Verdict> {
    let wanted = |s: SessionState| states.contains(&s.name());
    let gone = || {
        Some(if wanted(SessionState::Gone) {
            reached(session, SessionState::Gone)
        } else {
            Verdict::Never(status::Wait::Gone {
                session: session.to_owned(),
            })
        })
    };
    match step {
        Step::SessionStateChanged {
            session: s, state, ..
        } if s == session => {
            if *state == SessionState::Gone {
                gone()
            } else if wanted(*state) {
                Some(reached(session, *state))
            } else {
                None
            }
        }
        Step::SessionGone { session: s, .. } if s == session => gone(),
        Step::SessionEnded { session: s, .. } if s == session => {
            Some(Verdict::Never(status::Wait::Ended {
                session: session.to_owned(),
            }))
        }
        _ => None,
    }
}

/// One message: delivered, or never once dropped.
fn for_message(message: &str, step: &Step) -> Option<Verdict> {
    match step {
        Step::MessageDelivered {
            message: m,
            session,
            ..
        } if m == message => Some(Verdict::Delivered {
            session: session.clone(),
        }),
        Step::MessageDropped {
            message: m, why, ..
        } if m == message => Some(Verdict::Never(status::Wait::Dropped {
            message: message.to_owned(),
            why: *why,
        })),
        _ => None,
    }
}

fn reached(session: &str, state: SessionState) -> Verdict {
    Verdict::Reached {
        session: session.to_owned(),
        state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::Reason;
    use crate::messages::Dropped;
    use std::path::PathBuf;

    fn ev(id: i64, step: Step) -> Event {
        Event {
            id,
            occur_time_millis: 0,
            step,
        }
    }

    fn changed(session: &str, seat: Option<&str>, state: SessionState) -> Step {
        Step::SessionStateChanged {
            session: session.into(),
            fleet: Some("/c/app".into()),
            seat: seat.map(str::to_owned),
            state,
        }
    }

    fn gone_seat(session: &str) -> Step {
        Step::SessionGone {
            session: session.into(),
            fleet: "/c/app".into(),
            seat: "scribe".into(),
        }
    }

    fn scribe() -> Waited {
        Waited::Seat {
            fleet: PathBuf::from("/c/app"),
            seat: "scribe".into(),
        }
    }

    fn s1() -> Waited {
        Waited::Session {
            session: "s1".into(),
        }
    }

    #[test]
    fn a_wanted_state_of_the_session_is_reached() {
        let steps = [
            ev(1, changed("s1", None, SessionState::Working)),
            ev(2, changed("s1", None, SessionState::Idle)),
        ];
        assert_eq!(
            judge(&s1(), &[StateName::Idle], &steps),
            reached("s1", SessionState::Idle)
        );
        assert_eq!(judge(&s1(), &[StateName::Gone], &steps[..1]), Verdict::Keep);
    }

    #[test]
    fn needs_you_is_reached_whatever_its_reason() {
        let needs = SessionState::NeedsYou {
            reason: Some(Reason::Question),
        };
        assert_eq!(
            judge(
                &s1(),
                &[StateName::NeedsYou],
                &[ev(1, changed("s1", None, needs))]
            ),
            reached("s1", needs)
        );
    }

    #[test]
    fn a_seat_is_followed_across_its_sessions() {
        let steps = [
            ev(1, changed("s1", Some("scribe"), SessionState::Gone)),
            ev(2, gone_seat("s1")),
            ev(3, changed("s2", Some("scribe"), SessionState::Starting)),
            ev(4, changed("s2", Some("scribe"), SessionState::Idle)),
        ];
        assert_eq!(
            judge(&scribe(), &[StateName::Idle], &steps),
            reached("s2", SessionState::Idle)
        );
    }

    #[test]
    fn a_seats_gone_session_is_gone_reached_only_when_gone_is_wanted() {
        let gone = [ev(1, gone_seat("s1"))];
        assert_eq!(
            judge(&scribe(), &[StateName::Gone], &gone),
            reached("s1", SessionState::Gone)
        );
        assert_eq!(judge(&scribe(), &[StateName::Idle], &gone), Verdict::Keep);
    }

    #[test]
    fn a_session_gone_or_ended_can_never_reach_another_state() {
        let gone = [ev(1, changed("s1", None, SessionState::Gone))];
        assert_eq!(
            judge(&s1(), &[StateName::Idle], &gone),
            Verdict::Never(status::Wait::Gone {
                session: "s1".into()
            })
        );
        let ended = [ev(
            1,
            Step::SessionEnded {
                session: "s1".into(),
                fleet: None,
                seat: None,
            },
        )];
        assert_eq!(
            judge(&s1(), &[StateName::Gone], &ended),
            Verdict::Never(status::Wait::Ended {
                session: "s1".into()
            })
        );
    }

    #[test]
    fn a_seat_taken_out_or_its_fleet_removed_can_never_come() {
        let removed = [ev(
            1,
            Step::SeatRemoved {
                fleet: "/c/app".into(),
                seat: "scribe".into(),
            },
        )];
        assert_eq!(
            judge(&scribe(), &[StateName::Idle], &removed),
            Verdict::Never(status::Wait::SeatRemoved {
                fleet: "/c/app".into(),
                seat: "scribe".into()
            })
        );
        let gone = [ev(
            1,
            Step::FleetForgotten {
                directory: "/c/app".into(),
            },
        )];
        assert_eq!(
            judge(&scribe(), &[StateName::Idle], &gone),
            Verdict::Never(status::Wait::FleetRemoved {
                fleet: "/c/app".into()
            })
        );
    }

    #[test]
    fn another_fleets_seat_of_the_same_name_is_not_this_one() {
        let other = Step::SessionStateChanged {
            session: "s9".into(),
            fleet: Some("/c/api".into()),
            seat: Some("scribe".into()),
            state: SessionState::Idle,
        };
        assert_eq!(
            judge(&scribe(), &[StateName::Idle], &[ev(1, other)]),
            Verdict::Keep
        );
    }

    #[test]
    fn a_message_is_delivered_or_dropped() {
        let m1 = Waited::Message {
            message: "m1".into(),
        };
        let delivered = [ev(
            1,
            Step::MessageDelivered {
                message: "m1".into(),
                session: "s1".into(),
                fleet: None,
            },
        )];
        assert_eq!(
            judge(&m1, &[], &delivered),
            Verdict::Delivered {
                session: "s1".into()
            }
        );
        let dropped = [ev(
            1,
            Step::MessageDropped {
                message: "m1".into(),
                why: Dropped::SeatRemoved,
                fleet: None,
            },
        )];
        assert_eq!(
            judge(&m1, &[], &dropped),
            Verdict::Never(status::Wait::Dropped {
                message: "m1".into(),
                why: Dropped::SeatRemoved
            })
        );
    }

    #[test]
    fn the_first_step_that_decides_wins() {
        let steps = [
            ev(1, changed("s1", None, SessionState::Idle)),
            ev(
                2,
                Step::SessionEnded {
                    session: "s1".into(),
                    fleet: None,
                    seat: None,
                },
            ),
        ];
        assert_eq!(
            judge(&s1(), &[StateName::Idle], &steps),
            reached("s1", SessionState::Idle)
        );
    }
}
