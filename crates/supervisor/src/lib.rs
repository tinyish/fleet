//! The supervisor: one per user, the only writer of anything tinyfleet
//! writes. It answers the CLI on a Unix socket in the state directory.

use std::fmt;
use std::fs::{self, File, Permissions};
use std::io::{self, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use protocol::{Answer, Request};
use state::{State, Step};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug)]
enum Error {
    AlreadyRunning,
    Io(&'static str, io::Error),
    Protocol(protocol::ProtocolError),
    State(state::StateError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AlreadyRunning => f.write_str("already running for this state directory"),
            Error::Io(what, e) => write!(f, "{what}: {e}"),
            Error::Protocol(e) => write!(f, "{e}"),
            Error::State(e) => write!(f, "{e}"),
        }
    }
}

/// Runs the supervisor for `state_dir` until it is stopped.
#[must_use]
pub fn run(state_dir: &Path) -> ExitCode {
    match serve(state_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            log(&format!("fleet supervise: {e}"));
            ExitCode::FAILURE
        }
    }
}

fn log(line: &str) {
    let _ = writeln!(io::stderr(), "{line}");
}

fn serve(state_dir: &Path) -> Result<(), Error> {
    fs::create_dir_all(state_dir).map_err(|e| Error::Io("creating the state directory", e))?;
    fs::set_permissions(state_dir, Permissions::from_mode(0o700))
        .map_err(|e| Error::Io("securing the state directory", e))?;
    let lock = File::create(state_dir.join("supervisor.lock"))
        .map_err(|e| Error::Io("opening the lock", e))?;
    lock.try_lock().map_err(|_| Error::AlreadyRunning)?;

    let socket = protocol::socket_path(state_dir).map_err(Error::Protocol)?;
    match fs::remove_file(&socket) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(Error::Io("removing a stale socket", e)),
    }
    let listener = UnixListener::bind(&socket).map_err(|e| Error::Io("binding the socket", e))?;

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
                        log(&format!("fleet supervise: a request failed: {e}"));
                    }
                });
            }
            Err(e) => log(&format!("fleet supervise: accept failed: {e}")),
        }
    }
    drop(lock);
    Ok(())
}

fn answer(state: &Mutex<State>, stream: &UnixStream) -> Result<(), Error> {
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io("socket timeout", e))?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io("socket timeout", e))?;
    let reply = match protocol::read_request(&mut BufReader::new(stream)) {
        Ok(Request::Hello) => Answer::Hello {
            version: VERSION.to_owned(),
        },
        Ok(Request::AllOn) => record(state, &Step::EveryFleetOn, "every fleet is on")?,
        Ok(Request::AllOff) => record(state, &Step::EveryFleetOff, "every fleet is off")?,
        Err(e) => Answer::Refused {
            reason: e.to_string(),
        },
    };
    let mut writer = stream;
    protocol::write_answer(&mut writer, &reply).map_err(Error::Protocol)
}

fn record(state: &Mutex<State>, step: &Step, said: &str) -> Result<Answer, Error> {
    let mut state = state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    state.record(step).map_err(Error::State)?;
    Ok(Answer::Done {
        said: said.to_owned(),
    })
}
