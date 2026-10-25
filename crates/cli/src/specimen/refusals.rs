//! Every refusal the supervisor can answer with, as samples for the
//! specimen. Each builder matches its kind with no `_` arm, so a new refusal
//! does not compile until it has a sample here.

use fleet_core::protocol;

use super::samples::{API, APP, SRC, WEB, WING, bad_config, denied, path};

/// One refusal of `kind`; Delivery's, which may have several, come from
/// `delivery.rs`.
pub(super) fn refusals(kind: fleet_core::status::StatusKind) -> Vec<protocol::Status> {
    super::delivery::refusals(kind).unwrap_or_else(|| vec![refusal(kind)])
}

fn refusal(kind: fleet_core::status::StatusKind) -> protocol::Status {
    use fleet_core::protocol::Status as S;
    use fleet_core::status::StatusKind as K;
    match kind {
        K::DirectoryNotInRepo => S::DirectoryNotInRepo { path: path(WING) },
        K::DirectoryNotRepoRoot => S::DirectoryNotRepoRoot {
            path: path(SRC),
            root: path(APP),
        },
        K::ProjectNotInRepo => S::ProjectNotInRepo { path: path(WING) },
        K::ProjectNotRepoRoot => S::ProjectNotRepoRoot {
            path: path(SRC),
            root: path(APP),
        },
        K::ProjectIsFleetDirectory => S::ProjectIsFleetDirectory { path: path(APP) },
        K::WingmanInsideProject => S::WingmanInsideProject {
            directory: path(SRC),
            project: path(APP),
        },
        K::AlreadyHereServingOthers => S::AlreadyHereServingOthers {
            directory: path(WING),
            serving: vec![path(APP)],
            asked: vec![path(API)],
        },
        K::InsideFleet => S::InsideFleet {
            directory: path(SRC),
            fleet: path(APP),
        },
        K::ContainsFleet => S::ContainsFleet {
            directory: path("/code"),
            fleet: path(APP),
        },
        K::InsideServedProject => S::InsideServedProject {
            directory: path(SRC),
            project: path(APP),
            fleet: path(WING),
        },
        K::AlreadyServed => S::AlreadyServed {
            project: path(APP),
            fleet: path(WING),
        },
        K::Missing => S::Missing { path: path(API) },
        K::Unreadable => S::Unreadable {
            path: path(API),
            error: denied().to_string(),
        },
        K::ConfigDiffers => S::ConfigDiffers {
            path: path("/code/wing/fleet.toml"),
            listed: vec![path(API), path(WEB)],
        },
        K::CannotWrite => S::CannotWrite {
            path: path("/code/app/fleet.toml"),
            error: denied().to_string(),
        },
        K::BadConfig => bad_config(),
        K::AlsoClaimed => S::AlsoClaimed {
            project: path(API),
            fleet: path(WING),
        },
        K::NotServed => S::NotServed {
            path: path("/code/blog"),
        },
        K::InsideProject => S::InsideProject {
            path: path(SRC),
            project: path(APP),
        },
        K::NotFleetDirectory => S::NotFleetDirectory {
            directory: path(SRC),
            fleet: path(APP),
        },
        K::NoLongerServed => S::NoLongerServed {
            project: path(API),
            fleet: path(WING),
        },
        K::CannotTurnOn => S::CannotTurnOn {
            project: path(API),
            fleet: path(WING),
            why: Box::new(bad_config()),
        },
        K::Malformed => S::Malformed {
            error: "expected value at line 1 column 1".into(),
        },
        // Sampled in delivery.rs, which refusals() asks first.
        K::AdapterFailed
        | K::MultiplexerCouldNotRun
        | K::MultiplexerFailed
        | K::NoSuchSession
        | K::AdapterUnpinned
        | K::NotAnAdapter
        | K::NoSuchRelease
        | K::BadManifest
        | K::NoBuildFor
        | K::ChecksumMismatch
        | K::ProtocolMismatch
        | K::DownloadFailed
        | K::NoDownloader
        | K::AdapterNotKept
        | K::ReleaseMismatch => S::Malformed {
            error: format!("{kind:?} is sampled in delivery.rs"),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use strum::IntoEnumIterator;

    #[test]
    fn every_refusal_has_a_real_sample() {
        // A Delivery refusal routed to delivery.rs but given no sample
        // there would print the placeholder instead.
        for kind in fleet_core::status::StatusKind::iter() {
            let samples = refusals(kind);
            assert!(!samples.is_empty(), "{kind:?}");
            assert!(
                samples.iter().all(|s| !matches!(
                    s,
                    protocol::Status::Malformed { error } if error.ends_with("is sampled in delivery.rs")
                )),
                "{kind:?} has no sample in delivery.rs"
            );
        }
    }
}
