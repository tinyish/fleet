//! Samples of every step, as `fleet events` prints it, for the specimen.
//! No `_` arm: a new step does not compile until it has a sample here.

use fleet_core::agent::{Reason, SessionState};
use fleet_core::messages::{Dropped, Recipient, Sender};
use fleet_core::protocol::Event;
use fleet_core::protocol::response::ConfigFate;
use fleet_core::record::{Step, StepKind};
use strum::IntoEnumIterator;

use super::samples::{API, APP, WING, bad_config};

/// One event of each kind of step, and a few more where a step reads
/// differently, numbered from 1043 and recorded now.
pub(super) fn events() -> Vec<Event> {
    let now = jiff::Timestamp::now().as_millisecond();
    StepKind::iter()
        .flat_map(samples)
        .zip(1043..)
        .map(|(step, id)| Event {
            id,
            occur_time_millis: now,
            step,
        })
        .collect()
}

/// What events says as it starts: here, everywhere, and replaying.
pub(super) fn starts() -> Vec<String> {
    let ink = crate::ui::output::ERR;
    let app = std::path::Path::new(APP);
    vec![
        crate::ui::words::listening(Some(app), None, ink),
        crate::ui::words::listening(None, None, ink),
        crate::ui::words::listening(Some(app), Some(1042), ink),
    ]
}

fn s(text: &str) -> String {
    text.into()
}

/// Every sample of `kind`.
fn samples(kind: StepKind) -> Vec<Step> {
    use StepKind as K;
    match kind {
        K::SupervisorStarted => vec![Step::SupervisorStarted {
            version: s("0.2.0"),
            pid: 4242,
        }],
        K::EveryFleetOn => vec![Step::EveryFleetOn],
        K::EveryFleetOff => vec![Step::EveryFleetOff],
        K::FleetCreated => created(),
        K::ProjectOn => vec![Step::ProjectOn {
            project: s(APP),
            fleet: s(APP),
        }],
        K::ProjectOff => vec![Step::ProjectOff {
            project: s(APP),
            fleet: s(APP),
        }],
        K::ProjectAdded => vec![Step::ProjectAdded {
            fleet: s(WING),
            project: s(API),
        }],
        K::ProjectRemoved => vec![Step::ProjectRemoved {
            fleet: s(WING),
            project: s(API),
        }],
        K::FleetConfigBroken => vec![Step::FleetConfigBroken {
            fleet: s(WING),
            why: bad_config(),
        }],
        K::FleetRemoved => vec![Step::FleetRemoved {
            directory: s(APP),
            projects: vec![s(APP)],
            config: ConfigFate::Kept,
        }],
        K::ProjectsTaken => vec![Step::ProjectsTaken {
            fleet: s(WING),
            projects: vec![s(API)],
        }],
        K::FleetForgotten => vec![Step::FleetForgotten { directory: s(WING) }],
        K::SessionLaunched => launches(),
        K::SessionEnded => ends(),
        K::SeatAdded => vec![Step::SeatAdded {
            fleet: s(APP),
            seat: s("scribe"),
            role: s("writer"),
        }],
        K::SeatRemoved => vec![Step::SeatRemoved {
            fleet: s(APP),
            seat: s("scribe"),
        }],
        K::RoleChanged => vec![Step::RoleChanged {
            fleet: s(APP),
            role: s("writer"),
        }],
        K::SeatChanged => vec![Step::SeatChanged {
            fleet: s(APP),
            seat: s("scribe"),
            role: s("critic"),
        }],
        K::RoleAdded => vec![Step::RoleAdded {
            fleet: s(APP),
            role: s("critic"),
        }],
        K::RoleRemoved => vec![Step::RoleRemoved {
            fleet: s(APP),
            role: s("critic"),
        }],
        K::SessionGone => vec![Step::SessionGone {
            session: s("s1a2b3c4d5"),
            fleet: s(APP),
            seat: s("scribe"),
        }],
        K::SeatLaunchFailed => vec![Step::SeatLaunchFailed {
            fleet: s(APP),
            seat: s("scribe"),
            why: bad_config(),
        }],
        K::SessionStateChanged => states(),
        K::MessageQueued => messages(),
        K::MessageDelivered => vec![Step::MessageDelivered {
            message: s("m9f8e7d6c5"),
            session: s("s1a2b3c4d5"),
            fleet: Some(s(APP)),
        }],
        K::MessageDropped => drops(),
    }
}

/// A fleet made fresh, and one adopted from its fleet.toml.
fn created() -> Vec<Step> {
    [false, true]
        .map(|adopted| Step::FleetCreated {
            directory: s(WING),
            projects: vec![s(APP), s(API)],
            adopted,
        })
        .to_vec()
}

/// A seat's session ended, and a spawned one.
fn ends() -> Vec<Step> {
    [Some(s("scribe")), None]
        .map(|seat| Step::SessionEnded {
            session: s("s1a2b3c4d5"),
            fleet: Some(s(APP)),
            seat,
        })
        .to_vec()
}

/// A message dropped for each reason.
fn drops() -> Vec<Step> {
    [
        Dropped::SessionEnded,
        Dropped::SeatRemoved,
        Dropped::FleetRemoved,
    ]
    .map(|why| Step::MessageDropped {
        message: s("m9f8e7d6c5"),
        why,
        fleet: Some(s(APP)),
    })
    .to_vec()
}

/// A seat's launch, a spawn, and a session of neither.
fn launches() -> Vec<Step> {
    let launch = |seat: Option<&str>, role: Option<&str>| Step::SessionLaunched {
        session: s("s3fa01c2e9"),
        adapter: s("claude-code@2.1.285"),
        program: s("/state/adapters/claude-code/2.1.285/fleet-claude-code"),
        directory: s(APP),
        model: None,
        prompt: None,
        fleet: Some(s(APP)),
        seat: seat.map(s),
        role: role.map(s),
    };
    vec![
        launch(Some("scribe"), Some("writer")),
        launch(None, Some("worker")),
        launch(None, None),
    ]
}

/// A seat's session in each state, and a spawned one.
fn states() -> Vec<Step> {
    let changed = |seat: Option<&str>, state| Step::SessionStateChanged {
        session: s("s1a2b3c4d5"),
        fleet: Some(s(APP)),
        seat: seat.map(s),
        state,
    };
    [
        SessionState::Starting,
        SessionState::Working,
        SessionState::Idle,
        SessionState::NeedsYou { reason: None },
        SessionState::NeedsYou {
            reason: Some(Reason::Permission),
        },
        SessionState::Gone,
        SessionState::Unknown,
    ]
    .into_iter()
    .map(|state| changed(Some("scribe"), state))
    .chain([changed(None, SessionState::Idle)])
    .collect()
}

/// A message to a seat from a person, and to a session from a script and
/// from another agent.
fn messages() -> Vec<Step> {
    let queued = |to, sender| Step::MessageQueued {
        message: s("m9f8e7d6c5"),
        to,
        sender,
        text: s("write the release notes"),
        fleet: Some(s(APP)),
    };
    let to_session = || Recipient::Session {
        session: s("s1a2b3c4d5"),
    };
    vec![
        queued(
            Recipient::Seat {
                fleet: APP.into(),
                seat: s("scribe"),
            },
            Sender::Person,
        ),
        queued(to_session(), Sender::Script),
        queued(
            to_session(),
            Sender::Session {
                session: s("s3fa01c2e9"),
            },
        ),
    ]
}
