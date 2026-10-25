//! Every refusal the supervisor can answer with, as samples for the
//! specimen. Each builder matches its kind with no `_` arm, so a new refusal
//! does not compile until it has a sample here.

use fleet_core::agent::{AdapterFault, AdapterFaultKind, Verb};
use fleet_core::protocol;
use strum::IntoEnumIterator;

use super::samples::{API, APP, SRC, WEB, WING, bad_config, denied, not_found, path};

const CLAUDE: &str = "/code/fleet-adapters/target/debug/fleet-claude-code";

/// One refusal of `kind`, or, for an adapter's failure, one for every
/// fault under every verb.
pub(super) fn refusals(kind: fleet_core::status::StatusKind) -> Vec<protocol::Status> {
    if kind != fleet_core::status::StatusKind::AdapterFailed {
        return vec![refusal(kind)];
    }
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
        K::AdapterFailed => S::AdapterFailed {
            adapter: path(CLAUDE),
            verb: Verb::Launch,
            fault: AdapterFault::Exited {
                code: Some(1),
                said: "claude: not found".into(),
            },
        },
        K::MultiplexerCouldNotRun => S::MultiplexerCouldNotRun {
            error: not_found().to_string(),
        },
        K::MultiplexerFailed => S::MultiplexerFailed {
            command: "tmux new-session".into(),
            said: "can't use /dev/null".into(),
        },
        K::NoSuchSession => S::NoSuchSession {
            session: "s3f9a01c2d4".into(),
        },
    }
}
