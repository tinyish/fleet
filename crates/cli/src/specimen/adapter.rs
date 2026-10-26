//! Samples of what goes wrong naming and fetching adapters, for the
//! specimen: a protocol mismatch either way, and a release that does not
//! hold together each way. No `_` arm: a new refusal does not compile until
//! it has a sample here.

use fleet_core::status::{Adapter, AdapterKind};

use super::samples::{denied, not_found, path};

/// Every sample of `kind`.
pub(super) fn samples(kind: AdapterKind) -> Vec<Adapter> {
    use Adapter as A;
    use AdapterKind as K;
    match kind {
        K::AdapterUnpinned => vec![
            A::AdapterUnpinned {
                name: "claude-code".into(),
                latest: Some("2.1.285".into()),
            },
            A::AdapterUnpinned {
                name: "claude-code".into(),
                latest: None,
            },
        ],
        K::NotAnAdapter => vec![A::NotAnAdapter {
            reference: "claude-code@latest".into(),
        }],
        K::NoSuchRelease => vec![A::NoSuchRelease {
            name: "claude-code".into(),
            version: "9.9.9".into(),
        }],
        K::BadManifest => vec![A::BadManifest {
            url: RELEASE_URL.into(),
            error: "EOF while parsing a value at line 1 column 0".into(),
        }],
        K::NoBuildFor => vec![A::NoBuildFor {
            name: "claude-code".into(),
            version: "2.1.285".into(),
            target: "x86_64-unknown-linux-musl".into(),
        }],
        K::ChecksumMismatch => vec![A::ChecksumMismatch {
            name: "claude-code".into(),
            version: "2.1.285".into(),
            expected: "a32ee24651bfefaac911a7e65ad52d290c761da23173fca0f73ee94ff5633d81".into(),
            got: "9d704d58d1ba5455c2660e386244eda16273f75781336191821a9eae08c1baf7".into(),
        }],
        K::ProtocolMismatch => protocol_mismatches(),
        K::DownloadFailed => vec![A::DownloadFailed {
            url: RELEASE_URL.into(),
            said: "curl: (6) Could not resolve host: github.com".into(),
        }],
        K::NoDownloader => vec![A::NoDownloader {
            error: not_found().to_string(),
        }],
        K::ReleaseMismatch => mismatches(),
        K::AdapterNotKept => vec![A::AdapterNotKept {
            path: path("/home/me/.local/state/fleet/adapters/claude-code/2.1.285"),
            error: denied().to_string(),
        }],
    }
}

/// A release that does not hold together, each way.
fn mismatches() -> Vec<Adapter> {
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
    .map(|what| Adapter::ReleaseMismatch {
        url: RELEASE_URL.into(),
        what,
    })
    .collect()
}

const RELEASE_URL: &str = "https://github.com/tinyish/fleet-adapters/releases/download/claude-code-v2.1.285/manifest.json";

/// An adapter newer than this fleet, and one older.
fn protocol_mismatches() -> Vec<Adapter> {
    vec![
        Adapter::ProtocolMismatch {
            adapter: "claude-code@3.0.0".into(),
            speaks: 2,
            fleet_speaks: vec![1],
        },
        Adapter::ProtocolMismatch {
            adapter: "/code/mine/fleet-mine".into(),
            speaks: 1,
            fleet_speaks: vec![2, 3],
        },
    ]
}
