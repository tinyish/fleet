//! The supervisor: one per user, the only writer of anything tinyfleet
//! writes. It answers the CLI on a Unix socket in the state directory.

mod words;

use std::fs::{self, File, OpenOptions, Permissions};
use std::io::{self, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use fleet_core::config::{self, Config};
use fleet_core::fleets::{CreateDecision, decide_create};
use fleet_core::paths::{Os, state_dir};
use protocol::{ProtocolError, Reply, Request, Response, Status, request, response};
use state::{State, Step};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TIMEOUT: Duration = Duration::from_secs(2);

/// Why the supervisor stopped, or a request went unanswered; words.rs
/// words it for the log.
#[derive(Debug)]
enum Error {
    AlreadyRunning,
    Io(Doing, io::Error),
    Protocol(ProtocolError),
    SocketPathTooLong(PathBuf),
    State(state::StateError),
}

/// What the supervisor was doing when the system said no.
#[derive(Debug, Clone, Copy)]
enum Doing {
    CreatingStateDir,
    SecuringStateDir,
    OpeningLock,
    RemovingStaleSocket,
    BindingSocket,
    SettingTimeout,
}

/// Runs the supervisor for this machine's state directory until it is
/// stopped: what `fleet supervise` does.
#[must_use]
pub fn main() -> ExitCode {
    let Some(os) = Os::this() else {
        log(words::UNSUPPORTED_OS);
        return ExitCode::FAILURE;
    };
    match state_dir(os, |name| std::env::var_os(name)) {
        Ok(dir) => run(&dir),
        Err(e) => {
            log(&words::no_state_dir(&e));
            ExitCode::FAILURE
        }
    }
}

/// Runs the supervisor for `state_dir` until it is stopped.
#[must_use]
pub fn run(state_dir: &Path) -> ExitCode {
    match serve(state_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log(&words::stopped(&e));
            ExitCode::FAILURE
        }
    }
}

/// Writes one line to the supervisor's log (its stderr, which the service
/// manager keeps).
fn log(line: &str) {
    let _ = writeln!(io::stderr(), "{}{line}", words::PREFIX);
}

fn serve(state_dir: &Path) -> Result<(), Error> {
    fs::create_dir_all(state_dir).map_err(|e| Error::Io(Doing::CreatingStateDir, e))?;
    fs::set_permissions(state_dir, Permissions::from_mode(0o700))
        .map_err(|e| Error::Io(Doing::SecuringStateDir, e))?;
    let lock = File::create(state_dir.join("supervisor.lock"))
        .map_err(|e| Error::Io(Doing::OpeningLock, e))?;
    lock.try_lock().map_err(|_| Error::AlreadyRunning)?;

    let socket = protocol::socket_path(state_dir).map_err(|e| Error::SocketPathTooLong(e.0))?;
    match fs::remove_file(&socket) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io(Doing::RemovingStaleSocket, e)),
    }
    let listener = UnixListener::bind(&socket).map_err(|e| Error::Io(Doing::BindingSocket, e))?;

    let mut state = State::open(state_dir).map_err(Error::State)?;
    state
        .record(&Step::SupervisorStarted {
            version: VERSION.to_owned(),
            pid: std::process::id(),
        })
        .map_err(Error::State)?;
    let state = Arc::new(Mutex::new(state));

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    if let Err(e) = answer(&state, &stream) {
                        log(&words::request_failed(&e));
                    }
                });
            }
            Err(e) => log(&words::accept_failed(&e)),
        }
    }
    drop(lock);
    Ok(())
}

fn answer(state: &Mutex<State>, stream: &UnixStream) -> Result<(), Error> {
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io(Doing::SettingTimeout, e))?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io(Doing::SettingTimeout, e))?;
    let reply = match protocol::read_request(&mut BufReader::new(stream)) {
        Ok(Request::Hello) => Reply::Response(Response::Hello(response::Hello {
            version: VERSION.to_owned(),
        })),
        Ok(Request::Start(request::Start::All)) => record(
            state,
            &Step::EveryFleetOn,
            Response::Start(response::Start::AllOn),
        )?,
        Ok(Request::Stop(request::Stop::All)) => record(
            state,
            &Step::EveryFleetOff,
            Response::Stop(response::Stop::AllOff),
        )?,
        Ok(Request::Create(request::Create::Fleet {
            directory,
            projects,
        })) => create(state, &directory, &projects)?,
        // The parser's own detail; any other failure means no reply could
        // reach the other side, so it is only logged.
        Err(ProtocolError::Malformed(error)) => Reply::Status(Status::Malformed { error }),
        Err(e) => return Err(Error::Protocol(e)),
    };
    let mut writer = stream;
    protocol::write_reply(&mut writer, &reply).map_err(Error::Protocol)
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

fn record(state: &Mutex<State>, step: &Step, response: Response) -> Result<Reply, Error> {
    lock(state).record(step).map_err(Error::State)?;
    Ok(Reply::Response(response))
}

/// The root of the git repository containing `path`: the nearest directory
/// at or above it holding `.git` (a directory, or a file for a worktree).
fn repo_root_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Why a path could not be used: it does not exist, or it could not be read.
fn unusable(path: &Path, e: &io::Error) -> Reply {
    let path = path.to_path_buf();
    Reply::Status(if e.kind() == io::ErrorKind::NotFound {
        Status::Missing { path }
    } else {
        Status::Unreadable {
            path,
            error: e.to_string(),
        }
    })
}

/// Creates a fleet, under the state lock so that two creates never race.
fn create(state: &Mutex<State>, directory: &Path, projects: &[PathBuf]) -> Result<Reply, Error> {
    let directory = match fs::canonicalize(directory) {
        Ok(d) => d,
        Err(e) => return Ok(unusable(directory, &e)),
    };
    let mut canonical = Vec::with_capacity(projects.len());
    for project in projects {
        match fs::canonicalize(project) {
            Ok(p) => canonical.push(p),
            Err(e) => return Ok(unusable(project, &e)),
        }
    }
    let mut state = lock(state);
    let registered = state.fleets().map_err(Error::State)?;
    let fleet = match decide_create(&registered, &directory, &canonical, repo_root_of) {
        CreateDecision::Create(fleet) => fleet,
        CreateDecision::AlreadyHere(fleet) => {
            return Ok(Reply::Response(Response::Create(response::Create {
                directory: fleet.directory,
                projects: fleet.projects,
                created: false,
            })));
        }
        CreateDecision::Refuse(status) => return Ok(Reply::Status(status)),
    };
    let path = directory.join("fleet.toml");
    if path.exists() {
        return Ok(Reply::Status(Status::StrayConfig { path }));
    }
    let text = config::render(&Config::for_fleet(&fleet));
    let written = state.create_fleet(&fleet, || {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?
            .write_all(text.as_bytes())
    });
    match written {
        Ok(()) => {}
        Err(state::StateError::Config(e)) => {
            return Ok(Reply::Status(Status::CannotWrite {
                path,
                error: e.to_string(),
            }));
        }
        Err(e) => return Err(Error::State(e)),
    }
    state
        .record(&Step::FleetCreated {
            directory: directory.display().to_string(),
            projects: fleet
                .projects
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        })
        .map_err(Error::State)?;
    Ok(Reply::Response(Response::Create(response::Create {
        directory,
        projects: fleet.projects,
        created: true,
    })))
}
