//! Samples of what `fleet status` shows, for the specimen. `Overview` is a
//! struct, so the compiler cannot check these shapes: a fleet on with a seat
//! in each state (idle, working from an older role, needing you, down with a
//! reason and without, not running) and an orphan, with messages waiting
//! for the working seat; a fleet off with no seats; the sessions of no
//! fleet, one with a message waiting; an adapter that could not read; and
//! nothing at all.

use fleet_core::agent::{AdapterFault, Reason, SessionState, Verb};
use fleet_core::protocol::response::{
    Down, Fault, FleetOverview, Listed, Orphan, Overview, SeatOverview,
};
use fleet_core::status;

use super::samples::{APP, WING, path};

/// When the samples were answered, by the CLI's clock.
pub(super) const NOW: i64 = 1_793_000_000_000;

const CLAUDE: &str =
    "/Users/you/Library/Application Support/fleet/adapters/claude-code/2.1.285/fleet-claude-code";

fn listed(session: &str, state: SessionState) -> Listed {
    Listed {
        session: session.into(),
        adapter: path(CLAUDE),
        directory: path(APP),
        state,
        launch_time_millis: NOW - 600_000,
        waiting: 0,
    }
}

fn seat(name: &str, role: &str, session: Option<Listed>) -> SeatOverview {
    SeatOverview {
        name: name.into(),
        role: role.into(),
        session,
        older_role: false,
        down: None,
        waiting: 0,
    }
}

/// Every overview the specimen shows.
pub(super) fn overviews() -> Vec<Overview> {
    let failed = status::Session::AdapterFailed {
        adapter: path(CLAUDE),
        verb: Verb::Launch,
        fault: AdapterFault::Exited {
            code: Some(1),
            said: "claude: not found".into(),
        },
    };
    let on = FleetOverview {
        directory: path(APP),
        on: true,
        seats: vec![
            seat(
                "scribe",
                "writer",
                Some(listed("s1a2b3c4d5", SessionState::Idle)),
            ),
            SeatOverview {
                older_role: true,
                waiting: 2,
                ..seat(
                    "critic",
                    "review",
                    Some(listed("s9f8e7d6c5", SessionState::Working)),
                )
            },
            seat(
                "asker",
                "review",
                Some(listed(
                    "s5e4d3c2b1",
                    SessionState::NeedsYou {
                        reason: Some(Reason::Permission),
                    },
                )),
            ),
            SeatOverview {
                down: Some(Down {
                    why: Some(failed.into()),
                    next_try_millis: NOW + 40_000,
                }),
                ..seat("reader", "review", None)
            },
            SeatOverview {
                down: Some(Down {
                    why: None,
                    next_try_millis: NOW + 5_000,
                }),
                ..seat(
                    "flaky",
                    "writer",
                    Some(listed("s0a0b0c0d0", SessionState::Gone)),
                )
            },
            seat("new", "writer", None),
        ],
        orphans: vec![Orphan {
            seat: "old".into(),
            session: listed("s0011223344", SessionState::Idle),
        }],
    };
    let off = FleetOverview {
        directory: path(WING),
        on: false,
        seats: vec![],
        orphans: vec![],
    };
    vec![
        Overview {
            fleets: vec![on.clone()],
            ..Overview::default()
        },
        Overview {
            fleets: vec![on, off],
            loose: vec![Listed {
                waiting: 1,
                ..listed("s3f9a01c2d4", SessionState::Unknown)
            }],
            faults: vec![Fault {
                adapter: path(CLAUDE),
                fault: AdapterFault::TimedOut {
                    after_millis: 20_000,
                },
            }],
        },
        Overview::default(),
    ]
}
