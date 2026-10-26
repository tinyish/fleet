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

use super::refusals;
use crate::client;
use crate::protocol::{
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
pub(super) const SRC: &str = "/code/app/src";
pub(super) const WING: &str = "/code/wing";
pub(super) const API: &str = "/code/api";
pub(super) const WEB: &str = "/code/web";
const OLD: &str = "/code/old";
pub(super) const STATE: &str = "/Users/you/Library/Application Support/fleet";

pub(super) fn path(p: &str) -> PathBuf {
    PathBuf::from(p)
}

/// The system's own errors, built from their numbers so that they print as
/// a real one does (`Permission denied (os error 13)`). These numbers are the
/// same on macOS and Linux.
pub(super) fn denied() -> io::Error {
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
                        forgotten: if picked_up.is_empty() {
                            vec![]
                        } else {
                            vec![path(OLD)]
                        },
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
        ResponseKind::Cancelled => vec![Response::Cancelled],
        ResponseKind::Uninstalled => [false, true]
            .into_iter()
            .map(|kept_state_dir| Response::Uninstalled {
                binary: path("/Users/you/.local/bin/fleet"),
                state_dir: path(STATE),
                kept_state_dir,
            })
            .collect(),
        ResponseKind::Removed => removes()
            .into_iter()
            .flat_map(|response| {
                Startup::iter().map(move |startup| Response::Removed {
                    response: response.clone(),
                    startup,
                })
            })
            .collect(),
    }
}

/// Every way remove can end well: a fleet removed with its file deleted,
/// kept or already gone, a wingman fleet too, projects taken out one or
/// many, and nothing to remove.
fn removes() -> Vec<response::Remove> {
    use response::{ConfigFate, Remove};
    let mut all: Vec<Remove> = [ConfigFate::Deleted, ConfigFate::Kept, ConfigFate::Missing]
        .into_iter()
        .map(|config| Remove::Removed {
            directory: path(APP),
            projects: vec![path(APP)],
            config,
        })
        .collect();
    all.push(Remove::Removed {
        directory: path(WING),
        projects: vec![path(API), path(WEB)],
        config: ConfigFate::Kept,
    });
    for taken in [vec![path(WEB)], vec![path(API), path(WEB)]] {
        all.push(Remove::ProjectsTaken {
            directory: path(WING),
            taken,
            remaining: vec![path(APP)],
        });
    }
    all.push(Remove::Ask {
        directory: path(APP),
    });
    all.push(Remove::NothingHere {
        directory: path(SRC),
    });
    all
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
        forgotten: vec![],
    };
    if !broken {
        return vec![
            fine.clone(),
            response::AllOn {
                picked_up: picked_up(),
                ..fine.clone()
            },
            response::AllOn {
                forgotten: vec![path(OLD), path(WEB)],
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
            forgotten: vec![path(OLD)],
        },
        response::AllOn {
            on: vec![],
            fleets: vec![],
            picked_up: vec![],
            stayed_off: vec![
                off(WING, bad_config()),
                off(
                    APP,
                    fleet_core::status::File::Missing {
                        path: path("/code/app/fleet.toml"),
                    }
                    .into(),
                ),
            ],
            forgotten: vec![],
        },
    ]
}

pub(super) fn bad_config() -> protocol::Status {
    super::file::bad_config_sample().into()
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
            adopted: false,
        },
        response::Create {
            directory: path(WING),
            projects: vec![path(APP), path(API)],
            created: true,
            adopted: false,
        },
        response::Create {
            directory: path(APP),
            projects: vec![path(APP)],
            created: true,
            adopted: true,
        },
        response::Create {
            directory: path(WING),
            projects: vec![path(APP), path(API)],
            created: true,
            adopted: true,
        },
        response::Create {
            directory: path(APP),
            projects: vec![path(APP)],
            created: false,
            adopted: false,
        },
    ]
}

fn statuses(kind: StatusKind) -> Vec<(String, Status)> {
    match kind {
        StatusKind::Refused => fleet_core::status::StatusKind::iter()
            .flat_map(|area| {
                refusals::refusals(area).into_iter().map(move |(k, r)| {
                    (
                        format!("Status::Refused({area:?}::{k})"),
                        Status::Refused(r),
                    )
                })
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
        StatusKind::NeedsAnswer => vec![("Status::NeedsAnswer".into(), Status::NeedsAnswer)],
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
        ProblemKind::Uninstall => vec![Problem::Uninstall {
            path: path("/Users/you/.local/bin/fleet"),
            error: denied(),
        }],
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
        DoubtKind::StoppedUnanswered => vec![Doubt::StoppedUnanswered {
            waited: client::WAIT,
            log: format!("{STATE}/supervisor.log"),
        }],
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
            DuringKind::Removing => vec![During::Removing],
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
                labels
                    .iter()
                    .any(|l| l.starts_with(&format!("Status::Refused({kind:?}"))),
                "{kind:?}"
            );
        }
        for kind in crate::protocol::ResponseKind::iter() {
            assert!(
                labels
                    .iter()
                    .any(|l| l.starts_with(&format!("Response::{kind:?}"))),
                "{kind:?}"
            );
        }
    }

    #[test]
    fn every_refusal_kind_has_a_sample() {
        // An area whose samples are a Vec per arm could give none for a
        // kind and still compile: this catches it.
        use fleet_core::status::{AdapterKind, FileKind, FleetKind, ProjectKind, SessionKind};
        let labels: HashSet<String> = endings().into_iter().map(|s| s.label).collect();
        let kinds = FleetKind::iter()
            .map(|k| format!("Fleet::{k:?}"))
            .chain(ProjectKind::iter().map(|k| format!("Project::{k:?}")))
            .chain(FileKind::iter().map(|k| format!("File::{k:?}")))
            .chain(SessionKind::iter().map(|k| format!("Session::{k:?}")))
            .chain(AdapterKind::iter().map(|k| format!("Adapter::{k:?}")));
        for kind in kinds {
            assert!(
                labels.contains(&format!("Status::Refused({kind})")),
                "{kind} has no sample"
            );
        }
    }

    fn all_words() -> Vec<String> {
        endings()
            .iter()
            .map(|Sample { ending, .. }| {
                crate::ui::output::lines(ending)
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
                Ok(r) => crate::ui::words::response(r, crate::ui::output::OUT),
                Err(s) => crate::ui::words::status(s, crate::ui::output::ERR),
            };
            assert!(!text.trim().is_empty(), "{label}");
        }
    }
}
