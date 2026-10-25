//! Samples of Delivery's refusals for the specimen: sessions' and
//! adapters' faults, every adapter failure under every verb, and a protocol
//! mismatch either way.

use fleet_core::agent::{AdapterFault, AdapterFaultKind, Verb};
use fleet_core::protocol;
use fleet_core::status::StatusKind;
use strum::IntoEnumIterator;

use super::samples::{denied, not_found, path};

const CLAUDE: &str = "/code/fleet-adapters/target/debug/fleet-claude-code";

/// Every sample of `kind`, when it is one of Delivery's.
pub(super) fn refusals(kind: StatusKind) -> Option<Vec<protocol::Status>> {
    use StatusKind as K;
    use protocol::Status as S;
    Some(match kind {
        K::AdapterFailed => adapter_failures(),
        K::ProtocolMismatch => protocol_mismatches(),
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
        K::AdapterUnpinned => vec![
            S::AdapterUnpinned {
                name: "claude-code".into(),
                latest: Some("2.1.285".into()),
            },
            S::AdapterUnpinned {
                name: "claude-code".into(),
                latest: None,
            },
        ],
        K::NotAnAdapter => vec![S::NotAnAdapter {
            reference: "claude-code@latest".into(),
        }],
        K::NoSuchRelease => vec![S::NoSuchRelease {
            name: "claude-code".into(),
            version: "9.9.9".into(),
        }],
        K::BadManifest => vec![S::BadManifest {
            url: RELEASE_URL.into(),
            error: "EOF while parsing a value at line 1 column 0".into(),
        }],
        K::NoBuildFor => vec![S::NoBuildFor {
            name: "claude-code".into(),
            version: "2.1.285".into(),
            target: "x86_64-unknown-linux-musl".into(),
        }],
        K::ChecksumMismatch => vec![S::ChecksumMismatch {
            name: "claude-code".into(),
            version: "2.1.285".into(),
            expected: "a32ee24651bfefaac911a7e65ad52d290c761da23173fca0f73ee94ff5633d81".into(),
            got: "9d704d58d1ba5455c2660e386244eda16273f75781336191821a9eae08c1baf7".into(),
        }],
        K::DownloadFailed => vec![S::DownloadFailed {
            url: RELEASE_URL.into(),
            said: "curl: (6) Could not resolve host: github.com".into(),
        }],
        K::NoDownloader => vec![S::NoDownloader {
            error: not_found().to_string(),
        }],
        K::AdapterNotKept => vec![S::AdapterNotKept {
            path: path("/home/me/.local/state/fleet/adapters/claude-code/2.1.285"),
            error: denied().to_string(),
        }],
        K::ReleaseMismatch => mismatches(),
        _ => return None,
    })
}

/// A release that does not hold together, each way.
fn mismatches() -> Vec<protocol::Status> {
    use fleet_core::status::Mismatch as M;
    [
        M::OtherRelease {
            name: "codex".into(),
            version: "2.1.285".into(),
        },
        M::BuildSpeaks {
            manifest: 1,
            build: 2,
        },
        M::BuildMissing,
    ]
    .into_iter()
    .map(|what| protocol::Status::ReleaseMismatch {
        url: RELEASE_URL.into(),
        what,
    })
    .collect()
}

/// Every fault under every verb.
fn adapter_failures() -> Vec<protocol::Status> {
    Verb::ALL
        .into_iter()
        .flat_map(|verb| {
            AdapterFaultKind::iter()
                .flat_map(adapter_faults)
                .map(move |fault| protocol::Status::AdapterFailed {
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

const RELEASE_URL: &str = "https://github.com/tinyish/fleet-adapters/releases/download/claude-code-v2.1.285/manifest.json";

/// An adapter newer than this fleet, and one older.
fn protocol_mismatches() -> Vec<protocol::Status> {
    vec![
        protocol::Status::ProtocolMismatch {
            adapter: "claude-code@3.0.0".into(),
            speaks: 2,
            fleet_speaks: vec![1],
        },
        protocol::Status::ProtocolMismatch {
            adapter: "/code/mine/fleet-mine".into(),
            speaks: 1,
            fleet_speaks: vec![2, 3],
        },
    ]
}
