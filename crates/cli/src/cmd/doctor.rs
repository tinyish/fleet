//! `fleet doctor`: what tinyfleet runs on, each check fine, missing or
//! could not tell. It asks the drivers and the supervisor, reads the fleet
//! config and whether a state file is there, and starts and writes nothing.
//! Its findings are facts; `words/doctor.rs` words them.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use fleet_core::fleets;
use fleet_core::paths::{CONFIG_FILE, Os, PathError, state_dir};
use fleet_core::protocol::{self, ProtocolError, SocketPathTooLong};
use fleet_core::tools::{Git, Network, NoServiceManager, ServiceManager, ToolError};

use crate::client::{self, Hello};
use crate::protocol::{Problem, Status};
use crate::ui::output;

/// This binary's version, which a running supervisor should match.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What doctor checks, in the order it reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Checked {
    /// launchd or systemd.
    ServiceManager,
    /// git.
    Git,
    /// curl, which fetches adapters.
    Curl,
    /// The supervisor.
    Supervisor,
    /// The state file.
    State,
    /// The project the current directory is in, by its fleet's config.
    Project,
}

/// How one check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// In order.
    Fine,
    /// Fine for now, but worth noticing; it does not change the exit code.
    Warn,
    /// Missing or wrong.
    Missing,
    /// Doctor could not tell.
    CouldNotTell,
}

/// What doctor found. Each finding belongs to one check (`checked`).
#[derive(Debug)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(FoundKind), derive(strum::EnumIter))
)]
pub(crate) enum Found {
    /// The service manager answers.
    ManagerAnswers {
        /// Its name.
        name: &'static str,
    },
    /// The service manager does not answer.
    ManagerSilent {
        /// Its name.
        name: &'static str,
        /// What went wrong.
        error: ToolError,
    },
    /// The service manager cannot be found: `HOME` is not set.
    NoManager,
    /// git runs, and says its version.
    GitVersion(String),
    /// git does not run.
    GitBroken(ToolError),
    /// curl runs, and says its version.
    CurlVersion(String),
    /// curl does not run.
    CurlBroken(ToolError),
    /// The supervisor answers, at this binary's version.
    Running {
        /// Its version.
        version: String,
    },
    /// The supervisor answers at another version.
    OtherVersion {
        /// Its version.
        running: String,
        /// This binary's.
        ours: &'static str,
    },
    /// Nothing listens on this state directory's socket.
    NotRunning {
        /// The state directory the installed unit serves instead, if it
        /// serves another one.
        unit_serves: Option<PathBuf>,
    },
    /// Nothing listens, though this state directory's unit is installed.
    InstalledButSilent {
        /// Where its log can be read.
        log: String,
    },
    /// The supervisor did not answer in time.
    SupervisorSlow {
        /// How long doctor waited.
        waited: Duration,
        /// Where its log can be read, when the service manager says.
        log: Option<String>,
    },
    /// Talking to the supervisor failed.
    SupervisorFailed(ProtocolError),
    /// The supervisor answered something other than hello.
    OddHello,
    /// The socket path would be too long, so nothing can listen there.
    SocketTooLong(PathBuf),
    /// There is no state directory, so nowhere to look for a supervisor.
    NowhereToLook,
    /// The supervisor has the state open, at this schema.
    StateOpen {
        /// The state directory.
        dir: PathBuf,
        /// The schema version.
        schema: i64,
    },
    /// The running supervisor is too old to say its schema.
    StateUnsaid,
    /// Doctor could not tell how the state stands.
    StateUnknown,
    /// The state directory cannot be worked out.
    NoStateDir(PathError),
    /// The fleet the current directory is in reads, and every project is
    /// there.
    FleetPresent {
        /// The fleet directory.
        dir: PathBuf,
        /// How many projects it serves.
        projects: usize,
    },
    /// The fleet the current directory is in cannot be used.
    FleetBroken {
        /// The fleet directory.
        dir: PathBuf,
        /// Why.
        why: protocol::Status,
    },
    /// The current directory cannot be read.
    NoCurrentDir(io::Error),
}

impl Found {
    /// The check this finding belongs to.
    pub(crate) fn checked(&self) -> Checked {
        match self {
            Found::ManagerAnswers { .. } | Found::ManagerSilent { .. } | Found::NoManager => {
                Checked::ServiceManager
            }
            Found::GitVersion(_) | Found::GitBroken(_) => Checked::Git,
            Found::CurlVersion(_) | Found::CurlBroken(_) => Checked::Curl,
            Found::Running { .. }
            | Found::OtherVersion { .. }
            | Found::NotRunning { .. }
            | Found::InstalledButSilent { .. }
            | Found::SupervisorSlow { .. }
            | Found::SupervisorFailed(_)
            | Found::OddHello
            | Found::SocketTooLong(_)
            | Found::NowhereToLook => Checked::Supervisor,
            Found::StateOpen { .. }
            | Found::StateUnsaid
            | Found::StateUnknown
            | Found::NoStateDir(_) => Checked::State,
            Found::FleetPresent { .. } | Found::FleetBroken { .. } | Found::NoCurrentDir(_) => {
                Checked::Project
            }
        }
    }

    /// Whether this finding is fine, missing or something doctor could not
    /// tell.
    pub(crate) fn verdict(&self) -> Verdict {
        match self {
            Found::ManagerAnswers { .. }
            | Found::GitVersion(_)
            | Found::CurlVersion(_)
            | Found::Running { .. }
            | Found::StateOpen { .. }
            | Found::FleetPresent { .. } => Verdict::Fine,
            Found::NotRunning { .. } => Verdict::Warn,
            Found::ManagerSilent { .. }
            | Found::NoManager
            | Found::GitBroken(_)
            | Found::CurlBroken(_)
            | Found::OtherVersion { .. }
            | Found::InstalledButSilent { .. }
            | Found::SocketTooLong(_)
            | Found::NoStateDir(_)
            | Found::FleetBroken { .. } => Verdict::Missing,
            Found::SupervisorSlow { .. }
            | Found::SupervisorFailed(_)
            | Found::OddHello
            | Found::NowhereToLook
            | Found::StateUnsaid
            | Found::StateUnknown
            | Found::NoCurrentDir(_) => Verdict::CouldNotTell,
        }
    }
}

/// The exit code a script reads: 1 when anything is missing, otherwise 4
/// when anything could not be told, otherwise 0.
pub(crate) fn exit_code(report: &[Found]) -> u8 {
    let verdicts = || report.iter().map(Found::verdict);
    if verdicts().any(|v| v == Verdict::Missing) {
        1
    } else if verdicts().any(|v| v == Verdict::CouldNotTell) {
        4
    } else {
        0
    }
}

/// Runs every check, prints the report, and returns its exit code.
pub(crate) fn run(
    service: Result<&dyn ServiceManager, NoServiceManager>,
    git: &dyn Git,
    net: &dyn Network,
) -> ExitCode {
    let (Some(os), Ok(_) | Err(NoServiceManager::NoHome)) = (Os::this(), service) else {
        return output::report(&Err(Status::Cannot(Problem::UnsupportedOs)));
    };
    let dir = state_dir(os, |name| std::env::var_os(name));
    let report = check(service.ok(), git, net, &dir);
    output::doctor(&report);
    ExitCode::from(exit_code(&report))
}

/// Every check, in the order doctor reports them. `service` is `None` when
/// the service manager cannot be found. There is no state line while the
/// supervisor is not running, since only it opens the state, and no project
/// line outside a fleet directory.
fn check(
    service: Option<&dyn ServiceManager>,
    git: &dyn Git,
    net: &dyn Network,
    dir: &Result<PathBuf, PathError>,
) -> Vec<Found> {
    let installed = service.and_then(ServiceManager::installed_state_dir);
    let (supervisor, state) = match dir {
        Err(e) => (Found::NowhereToLook, Some(Found::NoStateDir(e.clone()))),
        Ok(dir) => match protocol::socket_path(dir) {
            Err(SocketPathTooLong(path)) => (Found::SocketTooLong(path), None),
            Ok(socket) => {
                let hello = client::hello(&socket);
                let state = state_found(dir, &hello);
                let log = service.map(|s| s.where_logs_go(dir));
                (supervisor_found(dir, installed.as_ref(), log, hello), state)
            }
        },
    };
    let mut report = vec![
        manager_found(service),
        git_found(git),
        curl_found(net),
        supervisor,
    ];
    report.extend(state);
    report.extend(project_found());
    report
}

fn manager_found(service: Option<&dyn ServiceManager>) -> Found {
    let Some(service) = service else {
        return Found::NoManager;
    };
    let name = service.name();
    match service.answers() {
        Ok(()) => Found::ManagerAnswers { name },
        Err(error) => Found::ManagerSilent { name, error },
    }
}

fn git_found(git: &dyn Git) -> Found {
    match git.version() {
        Ok(version) => Found::GitVersion(version),
        Err(error) => Found::GitBroken(error),
    }
}

fn curl_found(net: &dyn Network) -> Found {
    match net.version() {
        Ok(version) => Found::CurlVersion(version),
        Err(error) => Found::CurlBroken(error),
    }
}

fn supervisor_found(
    dir: &Path,
    installed: Option<&PathBuf>,
    log: Option<String>,
    hello: Hello,
) -> Found {
    match hello {
        Hello::Answered(hello) if hello.version == VERSION => Found::Running {
            version: hello.version,
        },
        Hello::Answered(hello) => Found::OtherVersion {
            running: hello.version,
            ours: VERSION,
        },
        Hello::NotListening => match (installed, log) {
            (Some(serves), Some(log)) if serves == dir => Found::InstalledButSilent { log },
            (Some(serves), _) if serves != dir => Found::NotRunning {
                unit_serves: Some(serves.clone()),
            },
            _ => Found::NotRunning { unit_serves: None },
        },
        Hello::TimedOut => Found::SupervisorSlow {
            waited: client::ONE,
            log,
        },
        Hello::Failed(e) => Found::SupervisorFailed(e),
        Hello::Odd => Found::OddHello,
    }
}

/// How the state stands, as a supervisor that answered says; nothing when
/// none is listening.
fn state_found(dir: &Path, hello: &Hello) -> Option<Found> {
    Some(match hello {
        Hello::Answered(protocol::response::Hello {
            schema_version: Some(schema),
            ..
        }) => Found::StateOpen {
            dir: dir.to_path_buf(),
            schema: *schema,
        },
        Hello::Answered(_) => Found::StateUnsaid,
        Hello::NotListening => return None,
        Hello::TimedOut | Hello::Failed(_) | Hello::Odd => Found::StateUnknown,
    })
}

/// The root of the git repository containing `path`: the nearest directory
/// at or above it holding `.git`.
fn repo_root_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The project the current directory is in, by the nearest fleet config at
/// or above it; nothing outside a fleet directory.
fn project_found() -> Option<Found> {
    let here = match std::env::current_dir() {
        Ok(here) => here,
        Err(e) => return Some(Found::NoCurrentDir(e)),
    };
    let dir = here.ancestors().find(|d| d.join(CONFIG_FILE).is_file())?;
    let read = fleets::read_fleet(
        dir,
        |p| fs::read_to_string(p),
        |p| fs::canonicalize(p),
        repo_root_of,
    );
    Some(match read {
        Ok((projects, _)) => Found::FleetPresent {
            dir: dir.to_path_buf(),
            projects: projects.len(),
        },
        Err(why) => Found::FleetBroken {
            dir: dir.to_path_buf(),
            why,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_misconfigured_state_directory_is_missing_not_unsure() {
        let relative = PathError::NotAbsolute {
            var: "FLEET_STATE_DIR",
            value: "relative".into(),
        };
        assert_eq!(Found::NoStateDir(relative).verdict(), Verdict::Missing);
        let long = Found::SocketTooLong("/very/long/supervisor.sock".into());
        assert_eq!(long.verdict(), Verdict::Missing);
        assert_eq!(Found::NowhereToLook.verdict(), Verdict::CouldNotTell);
        assert_eq!(Found::NowhereToLook.checked(), Checked::Supervisor);
    }

    #[test]
    fn before_a_first_start_one_thing_is_off_and_one_hint_says_how() {
        let dir = std::env::temp_dir().join(format!("fleet-first-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let report = check(Some(&Manager), &Git2, &Net2, &Ok(dir.clone()));
        let off: Vec<&Found> = report
            .iter()
            .filter(|f| f.verdict() != Verdict::Fine)
            .collect();
        assert!(
            matches!(off.as_slice(), [Found::NotRunning { .. }]),
            "{report:?}"
        );
        let hints: Vec<String> = report
            .iter()
            .filter_map(|f| crate::ui::words::found_next(f, crate::ui::output::OUT))
            .collect();
        assert_eq!(hints.len(), 1, "{hints:?}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_supervisor_not_running_warns_without_changing_the_exit_code() {
        let off = Found::NotRunning { unit_serves: None };
        assert_eq!(off.verdict(), Verdict::Warn);
        assert_eq!(exit_code(&[off]), 0);
    }

    struct Manager;
    impl ServiceManager for Manager {
        fn enable_and_start(&self, _: &fleet_core::tools::SupervisorUnit) -> Result<(), ToolError> {
            Ok(())
        }
        fn stop_and_disable(&self, _: &Path) -> Result<(), ToolError> {
            Ok(())
        }
        fn running(&self, _: &Path) -> bool {
            false
        }
        fn where_logs_go(&self, _: &Path) -> String {
            String::new()
        }
        fn name(&self) -> &'static str {
            "launchd"
        }
        fn answers(&self) -> Result<(), ToolError> {
            Ok(())
        }
        fn installed_state_dir(&self) -> Option<PathBuf> {
            None
        }
    }
    struct Git2;
    impl Git for Git2 {
        fn version(&self) -> Result<String, ToolError> {
            Ok("git version 2".into())
        }
    }
    struct Net2;
    impl Network for Net2 {
        fn download(&self, _: &str, _: &Path) -> Result<fleet_core::tools::Fetched, ToolError> {
            Ok(fleet_core::tools::Fetched::NotFound)
        }
        fn version(&self) -> Result<String, ToolError> {
            Ok("curl 8.7.1 (x86_64-apple-darwin23.0)".into())
        }
    }

    #[test]
    fn curl_is_reported_after_git_and_a_missing_one_is_missing() {
        let dir = std::env::temp_dir().join(format!("fleet-doctor-curl-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let report = check(Some(&Manager), &Git2, &Net2, &Ok(dir.clone()));
        let checks: Vec<Checked> = report.iter().map(Found::checked).collect();
        assert_eq!(checks.get(1..3), Some(&[Checked::Git, Checked::Curl][..]));
        assert!(
            report
                .iter()
                .any(|f| matches!(f, Found::CurlVersion(v) if v.starts_with("curl 8.7.1"))),
            "{report:?}"
        );
        let missing = Found::CurlBroken(ToolError::CouldNotRun {
            program: "curl".into(),
            error: io::Error::from(io::ErrorKind::NotFound),
        });
        assert_eq!(missing.verdict(), Verdict::Missing);
        assert_eq!(
            crate::ui::words::found_next(&missing, crate::ui::output::OUT.piped())
                .map(|w| console::strip_ansi_codes(&w).into_owned()),
            Some("install curl and put it on your PATH".into())
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn no_state_line_while_the_supervisor_is_not_running() {
        let dir = std::env::temp_dir().join(format!("fleet-doctor-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let report = check(Some(&Manager), &Git2, &Net2, &Ok(dir.clone()));
        let checks: Vec<Checked> = report.iter().map(Found::checked).collect();
        assert!(checks.contains(&Checked::Supervisor), "{report:?}");
        assert!(!checks.contains(&Checked::State), "{report:?}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn anything_missing_is_1_even_beside_could_not_tell() {
        let report = [
            Found::StateUnknown,
            Found::NotRunning { unit_serves: None },
            Found::GitBroken(ToolError::CouldNotRun {
                program: "git".into(),
                error: io::Error::from_raw_os_error(2),
            }),
        ];
        assert_eq!(exit_code(&report), 1);
    }

    #[test]
    fn could_not_tell_without_missing_is_4_and_all_fine_is_0() {
        let fine = Found::NotRunning { unit_serves: None };
        assert_eq!(exit_code(&[Found::StateUnknown, fine]), 4);
        assert_eq!(exit_code(&[Found::GitVersion("2".into())]), 0);
        assert_eq!(exit_code(&[]), 0);
    }
}
