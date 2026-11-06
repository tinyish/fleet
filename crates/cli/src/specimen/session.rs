//! Samples of what goes wrong with agents' sessions, for the specimen:
//! every adapter failure under every verb. No `_` arm: a new refusal does
//! not compile until it has a sample here.

use fleet_core::agent::{AdapterFault, AdapterFaultKind, Verb};
use fleet_core::protocol::response;
use fleet_core::status::{Session, SessionKind};
use strum::IntoEnumIterator;

use super::samples::{APP, WING, not_found, path};

const CLAUDE: &str = "/code/fleet-adapters/target/debug/fleet-claude-code";

/// Every sample of `kind`.
pub(super) fn samples(kind: SessionKind) -> Vec<Session> {
    use Session as S;
    use SessionKind as K;
    match kind {
        K::AdapterFailed => adapter_failures(),
        K::MultiplexerCouldNotRun => vec![S::MultiplexerCouldNotRun {
            error: not_found().to_string(),
        }],
        K::MultiplexerFailed => vec![S::MultiplexerFailed {
            command: "tmux new-session".into(),
            said: "can't use /dev/null".into(),
        }],
        K::LaunchTooLong => vec![S::LaunchTooLong {
            bytes: 20_514,
            limit: 16_000,
        }],
        K::NoFleetHere => vec![S::NoFleetHere {
            directory: path("/code/notes"),
        }],
        K::NoSuchRole => vec![S::NoSuchRole {
            role: "reviewer".into(),
            fleet: path(WING),
        }],
        K::NotAProject => vec![S::NotAProject {
            path: path("/code/app/src"),
            fleet: path(WING),
        }],
        K::SeatNotRunning => vec![S::SeatNotRunning {
            seat: "scribe".into(),
            fleet: path(APP),
        }],
    }
}

/// Every way `fleet spawn` ends done.
pub(super) fn spawns() -> Vec<crate::protocol::Response> {
    vec![crate::protocol::Response::Spawned(
        response::Spawn::Spawned {
            session: "s3fa01c2e9".into(),
            fleet: path(WING),
            role: "worker".into(),
            directory: path(APP),
        },
    )]
}

/// Every way `fleet end` ends done: a spawned session, a seat's, and one
/// already ended.
pub(super) fn ends() -> Vec<crate::protocol::Response> {
    [None, Some("scribe")]
        .into_iter()
        .map(|seat| response::End::Ended {
            session: "s3fa01c2e9".into(),
            seat: seat.map(str::to_owned),
        })
        .chain([response::End::AlreadyEnded {
            session: "s3fa01c2e9".into(),
        }])
        .map(crate::protocol::Response::Ended)
        .collect()
}

/// Every fault under every verb.
fn adapter_failures() -> Vec<Session> {
    Verb::ALL
        .into_iter()
        .flat_map(|verb| {
            AdapterFaultKind::iter()
                .flat_map(adapter_faults)
                .map(move |fault| Session::AdapterFailed {
                    adapter: path(CLAUDE),
                    verb,
                    fault,
                })
        })
        .collect()
}

fn adapter_faults(kind: AdapterFaultKind) -> Vec<AdapterFault> {
    match kind {
        AdapterFaultKind::CouldNotRun => vec![AdapterFault::CouldNotRun {
            error: not_found().to_string(),
        }],
        AdapterFaultKind::Exited => vec![
            AdapterFault::Exited {
                code: Some(1),
                said: "claude: not found".into(),
            },
            AdapterFault::Exited {
                code: None,
                said: String::new(),
            },
        ],
        AdapterFaultKind::TimedOut => vec![AdapterFault::TimedOut {
            after_millis: 20_000,
        }],
        AdapterFaultKind::BadAnswer => vec![AdapterFault::BadAnswer {
            error: "expected value at line 1 column 1".into(),
        }],
    }
}
