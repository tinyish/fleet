//! Samples of what goes wrong waiting, for the specimen. No `_` arm: a new
//! refusal does not compile until it has a sample here.

use std::time::Duration;

use fleet_core::agent::{Reason, SessionState, StateName};
use fleet_core::messages::Dropped;
use fleet_core::protocol::Waited;
use fleet_core::status::{Wait, WaitKind};

use super::samples::{APP, path};
use crate::protocol::{Response, Status};

fn scribe() -> Waited {
    Waited::Seat {
        fleet: path(APP),
        seat: "scribe".into(),
    }
}

fn session() -> Waited {
    Waited::Session {
        session: "s1a2b3c4d5".into(),
    }
}

fn message() -> Waited {
    Waited::Message {
        message: "m9f8e7d6c5".into(),
    }
}

/// What wait says once it waits: on a seat, a session with two states, a
/// message until delivered, and a message's answer.
pub(super) fn starts() -> Vec<String> {
    let ink = crate::ui::output::ERR;
    let idle = [StateName::Idle];
    vec![
        crate::ui::words::waiting(&scribe(), Some("s1a2b3c4d5"), &idle, ink),
        crate::ui::words::waiting(
            &session(),
            Some("s1a2b3c4d5"),
            &[StateName::Idle, StateName::NeedsYou],
            ink,
        ),
        crate::ui::words::waiting(&message(), None, &[], ink),
        crate::ui::words::waiting(&message(), None, &idle, ink),
    ]
}

/// A seat reached idle, a session that needs you, and one gone.
pub(super) fn reached() -> Vec<Response> {
    [
        (scribe(), SessionState::Idle),
        (
            session(),
            SessionState::NeedsYou {
                reason: Some(Reason::Question),
            },
        ),
        (session(), SessionState::Gone),
    ]
    .into_iter()
    .map(|(on, state)| Response::Reached {
        on,
        session: "s1a2b3c4d5".into(),
        state,
    })
    .collect()
}

/// A message typed in.
pub(super) fn delivered() -> Vec<Response> {
    vec![Response::Delivered {
        message: "m9f8e7d6c5".into(),
        session: "s1a2b3c4d5".into(),
    }]
}

/// A seat still working, a session not yet read, and a message not yet
/// typed in, each labelled.
pub(super) fn timed_out() -> Vec<(String, Status)> {
    [
        (
            scribe(),
            Some(SessionState::Working),
            Duration::from_mins(5),
        ),
        (session(), None, Duration::from_secs(90)),
        (message(), None, Duration::from_mins(90)),
    ]
    .into_iter()
    .map(|(on, state, after)| {
        let live = (!matches!(on, Waited::Message { .. })).then(|| "s1a2b3c4d5".to_owned());
        (
            "Status::TimedOut".to_owned(),
            Status::TimedOut {
                on,
                session: live,
                state,
                after,
            },
        )
    })
    .collect()
}

/// Every sample of `kind`.
pub(super) fn samples(kind: WaitKind) -> Vec<Wait> {
    use Wait as W;
    use WaitKind as K;
    match kind {
        K::NoSuchName => vec![
            W::NoSuchName {
                name: "critic".into(),
                fleet: Some(path(APP)),
            },
            W::NoSuchName {
                name: "s3f9a01c2d4".into(),
                fleet: None,
            },
        ],
        K::NoStates => vec![W::NoStates {
            name: "scribe".into(),
        }],
        K::Gone => vec![W::Gone {
            session: "s3f9a01c2d4".into(),
        }],
        K::Ended => vec![W::Ended {
            session: "s3f9a01c2d4".into(),
        }],
        K::SeatRemoved => vec![W::SeatRemoved {
            fleet: path(APP),
            seat: "scribe".into(),
        }],
        K::FleetRemoved => vec![W::FleetRemoved { fleet: path(APP) }],
        K::Dropped => [
            Dropped::SessionEnded,
            Dropped::SeatRemoved,
            Dropped::FleetRemoved,
        ]
        .into_iter()
        .map(|why| W::Dropped {
            message: "m9f8e7d6c5".into(),
            why,
        })
        .collect(),
    }
}
