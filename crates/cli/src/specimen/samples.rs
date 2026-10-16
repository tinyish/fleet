//! Every way a command can end, as samples for the specimen. Each builder
//! matches its kind with no `_` arm, so a new variant does not compile until
//! it has a sample here.

use fleet_core::protocol;
use std::ffi::OsStr;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

use fleet_core::paths::{PathError, PathErrorKind};
use fleet_core::protocol::{ProtocolError, ProtocolErrorKind, response};
use fleet_core::tools::{BadUnitPath, FileStep, ToolError, ToolErrorKind, UnitPath};
use strum::IntoEnumIterator;

use crate::client;
use crate::ending::{
    Doing, Doubt, DoubtKind, During, DuringKind, Problem, ProblemKind, Response, ResponseKind,
    Startup, Status, StatusKind,
};

/// One way a command can end, with the label the specimen shows above it.
pub(crate) struct Sample {
    /// What it is, as code names it.
    pub(crate) label: String,
    /// The ending itself.
    pub(crate) ending: Result<Response, Status>,
}

/// Every way a command can end: each `Response` kind, then each `Status`
/// kind, with nested facts expanded so that every value of every enum
/// appears at least once. Each builder matches its kind with no `_` arm, so
/// a new variant does not compile until it has a sample here.
pub(crate) fn endings() -> Vec<Sample> {
    let mut out = Vec::new();
    for kind in ResponseKind::iter() {
        for response in responses(kind) {
            out.push(Sample {
                label: format!("Response::{kind:?}"),
                ending: Ok(response),
            });
        }
    }
    for kind in StatusKind::iter() {
        for (label, status) in statuses(kind) {
            out.push(Sample {
                label,
                ending: Err(status),
            });
        }
    }
    out
}

pub(super) const APP: &str = "/code/app";
const SRC: &str = "/code/app/src";
pub(super) const WING: &str = "/code/wing";
const API: &str = "/code/api";
const WEB: &str = "/code/web";
pub(super) const STATE: &str = "/Users/you/Library/Application Support/fleet";

pub(super) fn path(p: &str) -> PathBuf {
    PathBuf::from(p)
}

/// The system's own errors, built from their numbers so that they print as
/// a real one does (`Permission denied (os error 13)`). These numbers are the
/// same on macOS and Linux.
fn denied() -> io::Error {
    io::Error::from_raw_os_error(13) // EACCES
}

pub(super) fn not_found() -> io::Error {
    io::Error::from_raw_os_error(2) // ENOENT
}

pub(super) fn broken_pipe() -> io::Error {
    io::Error::from_raw_os_error(32) // EPIPE
}

fn responses(kind: ResponseKind) -> Vec<Response> {
    match kind {
        ResponseKind::Created => creates()
            .into_iter()
            .flat_map(|response| {
                Startup::iter().map(move |startup| Response::Created {
                    response: response.clone(),
                    startup,
                })
            })
            .collect(),
        ResponseKind::Started => all_ons(false)
            .into_iter()
            .flat_map(|response| {
                Startup::iter().map(move |startup| Response::Started {
                    response: response.clone(),
                    startup,
                })
            })
            .collect(),
        ResponseKind::ProjectOn => [vec![], picked_up()]
            .into_iter()
            .flat_map(|picked_up| {
                Startup::iter().map(move |startup| Response::ProjectOn {
                    response: response::ProjectOn {
                        project: path(APP),
                        picked_up: picked_up.clone(),
                    },
                    startup,
                })
            })
            .collect(),
        ResponseKind::ProjectOff => [vec![], vec![path(API)], vec![path(API), path(WEB)]]
            .into_iter()
            .map(|others| {
                Response::ProjectOff(response::ProjectOff {
                    project: path(APP),
                    others,
                })
            })
            .collect(),
        ResponseKind::SupervisorStopped => vec![Response::SupervisorStopped],
        ResponseKind::SupervisorWasNotRunning => vec![Response::SupervisorWasNotRunning],
    }
}

/// What re-reading the configs can pick up: a project added to one fleet and
/// one removed from another.
fn picked_up() -> Vec<response::PickedUp> {
    vec![
        response::PickedUp {
            fleet: path(WING),
            added: vec![path(WEB)],
            removed: vec![],
        },
        response::PickedUp {
            fleet: path(APP),
            added: vec![],
            removed: vec![path(API)],
        },
    ]
}

/// What a plain start can answer. `response::AllOn` is a struct, so the
/// compiler cannot check these shapes: every fleet on, with what re-reading
/// picked up, and no fleets at all; with `broken`, some fleets stayed off,
/// one of them with nothing on.
fn all_ons(broken: bool) -> Vec<response::AllOn> {
    let fine = response::AllOn {
        on: vec![path(APP), path(API)],
        fleets: vec![path(APP), path(WING)],
        picked_up: vec![],
        stayed_off: vec![],
    };
    if !broken {
        return vec![
            fine.clone(),
            response::AllOn {
                picked_up: picked_up(),
                ..fine
            },
            response::AllOn::default(),
        ];
    }
    let off = |fleet: &str, why| response::StayedOff {
        fleet: path(fleet),
        why,
    };
    vec![
        response::AllOn {
            on: vec![path(APP)],
            fleets: vec![path(APP)],
            picked_up: vec![],
            stayed_off: vec![off(WING, bad_config())],
        },
        response::AllOn {
            on: vec![],
            fleets: vec![],
            picked_up: vec![],
            stayed_off: vec![
                off(WING, bad_config()),
                off(
                    APP,
                    protocol::Status::Missing {
                        path: path("/code/app/fleet.toml"),
                    },
                ),
            ],
        },
    ]
}

pub(super) fn bad_config() -> protocol::Status {
    protocol::Status::BadConfig {
        path: path("/code/wing/fleet.toml"),
        line: Some(1),
        column: Some(13),
        error: "unclosed array, expected `]`".into(),
    }
}

/// What create can answer. `response::Create` is a struct, so the compiler
/// cannot check that these shapes are all of them: a default fleet, a
/// wingman fleet, and a fleet that was already there.
fn creates() -> Vec<response::Create> {
    vec![
        response::Create {
            directory: path(APP),
            projects: vec![path(APP)],
            created: true,
        },
        response::Create {
            directory: path(WING),
            projects: vec![path(APP), path(API)],
            created: true,
        },
        response::Create {
            directory: path(APP),
            projects: vec![path(APP)],
            created: false,
        },
    ]
}

fn statuses(kind: StatusKind) -> Vec<(String, Status)> {
    match kind {
        StatusKind::Refused => fleet_core::status::StatusKind::iter()
            .map(|k| {
                (
                    format!("Status::Refused({k:?})"),
                    Status::Refused(refusal(k)),
                )
            })
            .collect(),
        StatusKind::Cannot => ProblemKind::iter()
            .flat_map(|k| {
                problems(k)
                    .into_iter()
                    .map(move |p| (format!("Status::Cannot({k:?})"), Status::Cannot(p)))
            })
            .collect(),
        StatusKind::NotRunning => vec![("Status::NotRunning".into(), Status::NotRunning)],
        StatusKind::PartlyOn => all_ons(true)
            .into_iter()
            .flat_map(|response| {
                Startup::iter().map(move |startup| {
                    (
                        "Status::PartlyOn".to_owned(),
                        Status::PartlyOn {
                            response: response.clone(),
                            startup,
                        },
                    )
                })
            })
            .collect(),
        StatusKind::CouldNotTell => DoubtKind::iter()
            .flat_map(|k| {
                doubts(k).into_iter().map(move |d| {
                    (
                        format!("Status::CouldNotTell({k:?})"),
                        Status::CouldNotTell(d),
                    )
                })
            })
            .collect(),
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
        K::StrayConfig => S::StrayConfig {
            path: path("/code/app/fleet.toml"),
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
    }
}

fn problems(kind: ProblemKind) -> Vec<Problem> {
    match kind {
        ProblemKind::UnsupportedOs => vec![Problem::UnsupportedOs],
        ProblemKind::StateDir => PathErrorKind::iter()
            .map(|k| {
                Problem::StateDir(match k {
                    PathErrorKind::NoHome => PathError::NoHome,
                    PathErrorKind::NotAbsolute => PathError::NotAbsolute {
                        var: "FLEET_STATE_DIR",
                        value: path("fleet-state"),
                    },
                })
            })
            .collect(),
        ProblemKind::SocketPathTooLong => vec![Problem::SocketPathTooLong(path(
            "/Users/you/a/very/deep/state/directory/that/goes/on/and/on/for/far/too/long/fleet/supervisor.sock",
        ))],
        ProblemKind::NoCurrentDir => vec![Problem::NoCurrentDir(denied())],
        ProblemKind::BadPath => vec![Problem::BadPath {
            path: path(API),
            error: denied(),
        }],
        ProblemKind::NoBinary => vec![Problem::NoBinary(denied())],
        ProblemKind::ServiceManager => {
            let mut out: Vec<Problem> = tool_errors()
                .into_iter()
                .map(|error| Problem::ServiceManager {
                    doing: Doing::Start,
                    error,
                })
                .collect();
            for doing in Doing::iter().filter(|d| *d != Doing::Start) {
                out.push(Problem::ServiceManager {
                    doing,
                    error: owned_elsewhere(),
                });
            }
            out
        }
    }
}

fn owned_elsewhere() -> ToolError {
    ToolError::OwnedElsewhere {
        theirs: path(STATE),
        ours: path("/tmp/fleet-test"),
    }
}

fn tool_errors() -> Vec<ToolError> {
    ToolErrorKind::iter()
        .flat_map(|kind| match kind {
            ToolErrorKind::OwnedElsewhere => vec![owned_elsewhere()],
            ToolErrorKind::CouldNotRun => vec![ToolError::CouldNotRun {
                program: "launchctl".into(),
                error: not_found(),
            }],
            ToolErrorKind::Failed => vec![
                ToolError::Failed {
                    command: "launchctl bootstrap gui/501 com.tinyish.fleet.supervisor.plist"
                        .into(),
                    said: "Bootstrap failed: 5: Input/output error".into(),
                },
                ToolError::Failed {
                    command: "systemctl --user daemon-reload".into(),
                    said: String::new(),
                },
            ],
            ToolErrorKind::File => FileStep::iter()
                .map(|doing| ToolError::File {
                    doing,
                    path: path(STATE),
                    error: denied(),
                })
                .collect(),
            ToolErrorKind::UnitPath => UnitPath::iter()
                .flat_map(|which| {
                    BadUnitPath::iter().map(move |why| ToolError::UnitPath {
                        which,
                        path: unit_path(which, why),
                        why,
                    })
                })
                .collect(),
        })
        .collect()
}

/// A path for `which` that really has the fault `why`: a newline inside it,
/// or a byte that is not UTF-8.
fn unit_path(which: UnitPath, why: BadUnitPath) -> PathBuf {
    let (dir, file) = match which {
        UnitPath::Program => ("/opt/fleet", "bin/fleet"),
        UnitPath::StateDir => ("/Users/you/fleet", "state"),
        UnitPath::Log => ("/Users/you/fleet/state", "supervisor.log"),
    };
    let mut bytes = dir.as_bytes().to_vec();
    match why {
        BadUnitPath::Newline => bytes.push(b'\n'),
        BadUnitPath::NotUtf8 => bytes.extend_from_slice(b"\xff/"),
    }
    bytes.extend_from_slice(file.as_bytes());
    PathBuf::from(OsStr::from_bytes(&bytes))
}

fn doubts(kind: DoubtKind) -> Vec<Doubt> {
    match kind {
        DoubtKind::NoAnswer => vec![
            Doubt::NoAnswer {
                waited: client::WAIT,
                log: format!("{STATE}/supervisor.log"),
            },
            Doubt::NoAnswer {
                waited: client::WAIT,
                log: "journalctl --user -u fleet-supervisor.service".into(),
            },
        ],
        DoubtKind::StillAnswers => vec![Doubt::StillAnswers {
            waited: client::WAIT,
        }],
        DoubtKind::Socket => {
            let mut out: Vec<Doubt> = durings()
                .into_iter()
                .map(|during| Doubt::Socket {
                    during,
                    error: ProtocolError::Empty,
                })
                .collect();
            for kind in ProtocolErrorKind::iter() {
                let error = match kind {
                    ProtocolErrorKind::Io => ProtocolError::Io(broken_pipe()),
                    ProtocolErrorKind::Malformed => {
                        ProtocolError::Malformed("expected value at line 1 column 1".into())
                    }
                    ProtocolErrorKind::Empty => continue,
                    ProtocolErrorKind::TooLong => ProtocolError::TooLong,
                };
                out.push(Doubt::Socket {
                    during: During::Creating,
                    error,
                });
            }
            out
        }
        DoubtKind::OddReply => vec![Doubt::OddReply { to: "create" }],
    }
}

fn durings() -> Vec<During> {
    DuringKind::iter()
        .flat_map(|kind| match kind {
            DuringKind::Creating => vec![During::Creating],
            DuringKind::TurningOn => Startup::iter().map(During::TurningOn).collect(),
            DuringKind::TurningOnProject => Startup::iter().map(During::TurningOnProject).collect(),
            DuringKind::TurningOff => vec![During::TurningOff],
            DuringKind::TurningOffProject => vec![During::TurningOffProject],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_kind_has_at_least_one_sample() {
        let labels: HashSet<String> = endings().into_iter().map(|s| s.label).collect();
        for kind in fleet_core::status::StatusKind::iter() {
            assert!(
                labels.contains(&format!("Status::Refused({kind:?})")),
                "{kind:?}"
            );
        }
        for kind in crate::ending::ResponseKind::iter() {
            assert!(
                labels
                    .iter()
                    .any(|l| l.starts_with(&format!("Response::{kind:?}"))),
                "{kind:?}"
            );
        }
    }

    fn all_words() -> Vec<String> {
        endings()
            .iter()
            .map(|Sample { ending, .. }| {
                crate::output::lines(ending)
                    .into_iter()
                    .map(|(_, text)| text)
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .collect()
    }

    #[test]
    fn io_samples_read_as_the_system_writes_them() {
        let words = all_words();
        assert!(
            words
                .iter()
                .any(|w| w.contains("Permission denied (os error 13)"))
        );
        for kind_text in ["permission denied", "entity not found", "connection reset"] {
            assert!(!words.iter().any(|w| w.contains(kind_text)), "{kind_text}");
        }
    }

    #[test]
    fn unit_path_samples_carry_their_fault() {
        let mut paths = Vec::new();
        for error in tool_errors() {
            if let ToolError::UnitPath { path, why, .. } = error {
                match why {
                    BadUnitPath::Newline => assert!(path.to_string_lossy().contains('\n')),
                    BadUnitPath::NotUtf8 => assert!(path.to_str().is_none()),
                }
                paths.push(path);
            }
        }
        let distinct: HashSet<_> = paths.iter().collect();
        assert_eq!(distinct.len(), paths.len(), "one path per which and fault");
    }

    #[test]
    fn every_sample_has_words() {
        for Sample { label, ending } in endings() {
            let text = match &ending {
                Ok(r) => crate::words::response(r, crate::output::OUT),
                Err(s) => crate::words::status(s, crate::output::ERR),
            };
            assert!(!text.trim().is_empty(), "{label}");
        }
    }
}
