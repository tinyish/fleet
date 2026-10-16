//! `fleet specimen`: every line the CLI can print, through the same code a
//! command uses, so a person can look at all of them on a real terminal.
//! Compiled only with the `specimen` feature, so a release build has no such
//! command. Its labels are code identifiers, never sentences.

use std::ffi::OsStr;
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::process::ExitCode;
use std::thread;
use std::time::Duration;

use clap::{CommandFactory, Parser};
use cliclack::{Theme, ThemeState};

use fleet_core::paths::{PathError, PathErrorKind};
use fleet_core::tools::{BadUnitPath, FileStep, ToolError, ToolErrorKind, UnitPath};
use protocol::{ProtocolError, ProtocolErrorKind, response};
use strum::IntoEnumIterator;

use crate::ending::{
    Doing, Doubt, DoubtKind, During, DuringKind, Problem, ProblemKind, Response, ResponseKind,
    Startup, Status, StatusKind,
};
use crate::theme::{FleetTheme, Tone, paint};
use crate::{Args, client, output, ui, words};

/// Prints every line: the help text, the start prompt in each state, the
/// spinners, then every way a command can end with its hint and exit code.
pub(crate) fn run() -> ExitCode {
    help();
    prompts();
    spinners();
    for Sample { label, ending } in endings() {
        let code = match &ending {
            Ok(_) => 0,
            Err(status) => status.exit_code(),
        };
        heading(&format!("{label} · exit {code}"));
        let _ = output::report(&ending);
        let _ = io::stdout().flush();
    }
    ExitCode::SUCCESS
}

/// A gray label on stderr, after flushing stdout so that a piped `2>&1`
/// keeps each label above its own lines.
fn heading(label: &str) {
    let _ = io::stdout().flush();
    let _ = writeln!(
        io::stderr(),
        "\n{}",
        paint(Tone::Muted).apply_to(format!("── {label}"))
    );
}

/// The help and version text exactly as clap prints it, by asking the real
/// parser for it.
fn help() {
    for argv in help_argvs() {
        heading(&argv.join(" "));
        if let Err(e) = Args::try_parse_from(&argv) {
            let _ = e.print();
        }
    }
}

/// The command lines whose help, version or usage text the specimen shows:
/// bare `fleet`, which prints help as an error, each `--help`, `--version`,
/// and one usage error.
fn help_argvs() -> Vec<Vec<String>> {
    let mut argvs = vec![
        vec!["fleet".to_owned()],
        vec!["fleet".to_owned(), "--help".to_owned()],
    ];
    argvs.push(vec!["fleet".to_owned(), "--version".to_owned()]);
    for sub in Args::command().get_subcommands() {
        argvs.push(vec![
            "fleet".to_owned(),
            sub.get_name().to_owned(),
            "--help".to_owned(),
        ]);
    }
    argvs.push(vec![
        "fleet".to_owned(),
        "create".to_owned(),
        "--bogus".to_owned(),
    ]);
    argvs
}

/// The start prompt in each state, drawn by the same theme calls cliclack
/// makes. `ThemeState::Error` is left out: a yes/no prompt never validates.
fn prompts() {
    let frames = [
        ("ThemeState::Active, Yes", ThemeState::Active, true),
        ("ThemeState::Active, No", ThemeState::Active, false),
        ("ThemeState::Submit, Yes", ThemeState::Submit, true),
        ("ThemeState::Submit, No", ThemeState::Submit, false),
        ("ThemeState::Cancel", ThemeState::Cancel, true),
    ];
    for (label, state, value) in frames {
        heading(&format!("ui::confirm · {label}"));
        let theme = FleetTheme;
        let _ = write!(
            io::stderr(),
            "{}{}{}",
            theme.format_header(&state, words::START_PROMPT),
            theme.format_confirm(&state, value),
            theme.format_footer(&state)
        );
    }
}

/// Each spinner, running for a moment. Piped, it draws nothing, as in a
/// real command.
fn spinners() {
    for message in [words::STARTING, words::STOPPING] {
        heading("ui::spinner");
        let spinner = ui::spinner(message);
        thread::sleep(Duration::from_millis(1500));
        spinner.clear();
    }
}

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

const APP: &str = "/code/app";
const SRC: &str = "/code/app/src";
const WING: &str = "/code/wing";
const API: &str = "/code/api";
const STATE: &str = "/Users/you/Library/Application Support/fleet";

fn path(p: &str) -> PathBuf {
    PathBuf::from(p)
}

/// The system's own errors, built from their numbers so that they print as
/// a real one does (`Permission denied (os error 13)`). These numbers are the
/// same on macOS and Linux.
fn denied() -> io::Error {
    io::Error::from_raw_os_error(13) // EACCES
}

fn not_found() -> io::Error {
    io::Error::from_raw_os_error(2) // ENOENT
}

fn broken_pipe() -> io::Error {
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
        ResponseKind::Started => response::Start::iter()
            .flat_map(|response| {
                Startup::iter().map(move |startup| Response::Started {
                    response: response.clone(),
                    startup,
                })
            })
            .collect(),
        ResponseKind::SupervisorStopped => vec![Response::SupervisorStopped],
        ResponseKind::SupervisorWasNotRunning => vec![Response::SupervisorWasNotRunning],
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
    use fleet_core::status::StatusKind as K;
    use protocol::Status as S;
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
            DuringKind::TurningOff => vec![During::TurningOff],
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
            .map(|Sample { ending, .. }| match ending {
                Ok(r) => crate::words::response(r, crate::output::OUT),
                Err(s) => crate::words::status(s, crate::output::ERR),
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
    fn help_shows_bare_fleet_and_a_usage_error() {
        let argvs = help_argvs();
        assert!(argvs.iter().any(|a| a == &["fleet"]));
        assert!(argvs.iter().any(|a| a.iter().any(|x| x == "--bogus")));
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
