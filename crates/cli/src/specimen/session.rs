//! Samples of what goes wrong with agents' sessions, for the specimen:
//! every adapter failure under every verb. No `_` arm: a new refusal does
//! not compile until it has a sample here.

use fleet_core::agent::{AdapterFault, AdapterFaultKind, Verb};
use fleet_core::status::{Session, SessionKind};
use strum::IntoEnumIterator;

use super::samples::{not_found, path};

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
        K::NoSuchSession => vec![S::NoSuchSession {
            session: "s3f9a01c2d4".into(),
        }],
    }
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
