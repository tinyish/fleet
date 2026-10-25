//! Every finding `fleet doctor` can report, as samples for the specimen.
//! `findings` matches each kind with no `_` arm, so a new finding does not
//! compile until it has a sample here.

use fleet_core::paths::PathError;
use fleet_core::protocol::ProtocolError;
use fleet_core::tools::ToolError;
use strum::IntoEnumIterator;

use super::samples::{APP, STATE, WING, bad_config, broken_pipe, not_found, path};
use crate::client;
use crate::cmd::doctor::{Found, FoundKind};

/// Every finding doctor can report, each labelled with its kind.
pub(crate) fn doctor() -> Vec<(String, Found)> {
    FoundKind::iter()
        .flat_map(|kind| {
            findings(kind)
                .into_iter()
                .map(move |found| (format!("doctor · Found::{kind:?}"), found))
        })
        .collect()
}

fn findings(kind: FoundKind) -> Vec<Found> {
    let log = format!("{STATE}/supervisor.log");
    match kind {
        FoundKind::ManagerAnswers => vec![Found::ManagerAnswers { name: "launchd" }],
        FoundKind::ManagerSilent => vec![manager_silent()],
        FoundKind::NoManager => vec![Found::NoManager],
        FoundKind::GitVersion => vec![Found::GitVersion("git version 2.50.1".into())],
        FoundKind::GitBroken => git_broken(),
        FoundKind::CurlVersion => vec![Found::CurlVersion("curl 8.7.1".into())],
        FoundKind::CurlBroken => curl_broken(),
        FoundKind::Running => vec![Found::Running {
            version: "0.1.0".into(),
        }],
        FoundKind::OtherVersion => vec![Found::OtherVersion {
            running: "0.0.9".into(),
            ours: "0.1.0",
        }],
        FoundKind::NotRunning => vec![
            Found::NotRunning { unit_serves: None },
            Found::NotRunning {
                unit_serves: Some(path(STATE)),
            },
        ],
        FoundKind::InstalledButSilent => vec![Found::InstalledButSilent { log: log.clone() }],
        FoundKind::SupervisorSlow => vec![
            Found::SupervisorSlow {
                waited: client::ONE,
                log: Some(log),
            },
            Found::SupervisorSlow {
                waited: client::ONE,
                log: None,
            },
        ],
        FoundKind::SupervisorFailed => {
            vec![Found::SupervisorFailed(ProtocolError::Io(broken_pipe()))]
        }
        FoundKind::OddHello => vec![Found::OddHello],
        FoundKind::SocketTooLong => vec![Found::SocketTooLong(path(
            "/Users/you/a/very/deep/state/directory/supervisor.sock",
        ))],
        FoundKind::NowhereToLook => vec![Found::NowhereToLook],
        FoundKind::StateOpen => vec![Found::StateOpen {
            dir: path(STATE),
            schema: 1,
        }],
        FoundKind::StateUnsaid => vec![Found::StateUnsaid],
        FoundKind::StateUnknown => vec![Found::StateUnknown],
        FoundKind::NoStateDir => vec![
            Found::NoStateDir(PathError::NoHome),
            Found::NoStateDir(PathError::NotAbsolute {
                var: "FLEET_STATE_DIR",
                value: path("fleet-state"),
            }),
        ],
        FoundKind::FleetPresent => [(APP, 1), (WING, 2)]
            .into_iter()
            .map(|(dir, projects)| Found::FleetPresent {
                dir: path(dir),
                projects,
            })
            .collect(),
        FoundKind::FleetBroken => vec![Found::FleetBroken {
            dir: path(WING),
            why: bad_config(),
        }],
        FoundKind::NoCurrentDir => vec![Found::NoCurrentDir(not_found())],
    }
}

fn manager_silent() -> Found {
    let error = ToolError::Failed {
        command: "launchctl print gui/501".into(),
        said: "Could not find domain for port identifier".into(),
    };
    Found::ManagerSilent {
        name: "launchd",
        error,
    }
}

fn git_broken() -> Vec<Found> {
    let missing = ToolError::CouldNotRun {
        program: "git".into(),
        error: not_found(),
    };
    let failed = ToolError::Failed {
        command: "git --version".into(),
        said: "xcrun: error: invalid active developer path".into(),
    };
    vec![Found::GitBroken(missing), Found::GitBroken(failed)]
}

fn curl_broken() -> Vec<Found> {
    let missing = ToolError::CouldNotRun {
        program: "curl".into(),
        error: not_found(),
    };
    let failed = ToolError::Failed {
        command: "curl --version".into(),
        said: "dyld: Library not loaded: libcurl.4.dylib".into(),
    };
    vec![Found::CurlBroken(missing), Found::CurlBroken(failed)]
}
