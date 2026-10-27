//! The supervisor: one per user, the only writer of anything tinyfleet
//! writes. It answers the CLI on a Unix socket in the state directory.

mod adapters;
mod create;
mod messages;
mod observe;
mod overview;
mod projects;
mod remove;
mod sessions;
mod tending;
#[cfg(test)]
mod testing;
mod words;

use fleet_core::protocol;
use std::ffi::OsString;
use std::fmt::Write as _;
use std::fs::{self, File, Permissions};
use std::io::{self, BufReader, Read as _, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use fleet_core::paths::{Os, state_dir};
use fleet_core::protocol::{ProtocolError, Reply, Request, Response, Status, request, response};
use fleet_core::tools::{AgentAdapters, Multiplexer, Network};
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

impl From<state::StateError> for Error {
    fn from(e: state::StateError) -> Self {
        Error::State(e)
    }
}

/// The outside tools the supervisor drives, chosen by the binary.
pub struct Tools {
    /// tmux, on tinyfleet's own server.
    pub mux: Box<dyn Multiplexer + Send + Sync>,
    /// The agent adapters, run by the host that runs adapter programs.
    pub agents: Box<dyn AgentAdapters + Send + Sync>,
    /// The network, which fetches adapters named by a release.
    pub net: Box<dyn Network + Send + Sync>,
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
/// stopped: what `fleet supervise` does. `args` are the arguments after
/// `supervise`; it takes none, and refuses any as a usage error (exit 2).
/// `tools` gives the outside tools for the state directory.
#[must_use]
pub fn main(
    args: impl IntoIterator<Item = OsString>,
    tools: impl FnOnce(&Path) -> Tools,
) -> ExitCode {
    if let Some(arg) = args.into_iter().next() {
        log(&words::takes_no_arguments(&arg));
        return ExitCode::from(2);
    }
    let Some(os) = Os::this() else {
        log(words::UNSUPPORTED_OS);
        return ExitCode::FAILURE;
    };
    match state_dir(os, |name| std::env::var_os(name)) {
        Ok(dir) => {
            let tools = tools(&dir);
            run(&dir, tools)
        }
        Err(e) => {
            log(&words::no_state_dir(&e));
            ExitCode::FAILURE
        }
    }
}

/// Runs the supervisor for `state_dir`, driving `tools`, until it is
/// stopped.
#[must_use]
pub fn run(state_dir: &Path, tools: Tools) -> ExitCode {
    match serve(state_dir, tools) {
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

fn serve(state_dir: &Path, tools: Tools) -> Result<(), Error> {
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

    let mut state = State::open(state_dir)?;
    let started = Step::SupervisorStarted {
        version: VERSION.to_owned(),
        pid: std::process::id(),
    };
    state.write(|w| w.record(&started))?;
    let state = Arc::new(Mutex::new(state));
    let tools = Arc::new(tools);
    let store = Arc::new(adapters::Store::new(
        state_dir,
        std::env::var(fleet_core::adapters::BASE_VAR).ok(),
    ));
    let tending = Arc::new(tending::Tending::new());
    {
        let (state, tools, store, tending) = (
            Arc::clone(&state),
            Arc::clone(&tools),
            Arc::clone(&store),
            Arc::clone(&tending),
        );
        thread::spawn(move || tending::run(&state, &tools, &store, &tending));
    }

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let state = Arc::clone(&state);
                let tools = Arc::clone(&tools);
                let store = Arc::clone(&store);
                let tending = Arc::clone(&tending);
                thread::spawn(move || {
                    if let Err(e) = answer(&state, &tools, &store, &tending, &stream) {
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

fn answer(
    state: &Mutex<State>,
    tools: &Tools,
    store: &adapters::Store,
    tending: &tending::Tending,
    stream: &UnixStream,
) -> Result<(), Error> {
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io(Doing::SettingTimeout, e))?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io(Doing::SettingTimeout, e))?;
    let reply = match protocol::read_request(&mut BufReader::new(stream)) {
        Ok(Request::Hello) => Reply::Response(Response::Hello(response::Hello {
            version: VERSION.to_owned(),
            schema_version: Some(state::SCHEMA_VERSION),
        })),
        Ok(Request::Start(request::Start::All)) => projects::all_on(state)?,
        Ok(Request::Start(request::Start::Project { project })) => {
            projects::project_on(state, &project)?
        }
        Ok(Request::Stop(request::Stop::All)) => projects::all_off(state)?,
        Ok(Request::Stop(request::Stop::Project { project })) => {
            projects::project_off(state, &project)?
        }
        Ok(Request::Create(request::Create::Fleet {
            directory,
            projects,
        })) => create::create(state, &directory, &projects)?,
        Ok(Request::Remove(request::Remove::Fleet {
            directory,
            projects,
            config,
        })) => remove::remove(state, &directory, &projects, config)?,
        Ok(Request::Session(request::Session::Launch {
            adapter,
            directory,
            model,
            prompt,
        })) => sessions::launch(
            state,
            tools,
            store,
            &sessions::Ask {
                adapter,
                directory,
                model,
                prompt,
            },
        )?,
        Ok(Request::Overview { directory }) => {
            overview::overview(state, tools, tending, &directory)?
        }
        Ok(Request::Session(request::Session::Stop { session })) => {
            sessions::stop(state, tools, &session)?
        }
        Ok(Request::Message(request::Message::Send {
            directory,
            to,
            text,
            sender,
        })) => messages::send(
            state,
            tools,
            &messages::Send {
                directory,
                to,
                text,
                sender,
            },
        )?,
        // The parser's own detail; any other failure means no reply could
        // reach the other side, so it is only logged.
        Err(ProtocolError::Malformed(error)) => Reply::Status(Status::Malformed { error }),
        Err(e) => return Err(Error::Protocol(e)),
    };
    // Start and create may have turned projects on or read seats, and a
    // message may wait for an idle agent: tend now.
    if let Reply::Response(Response::Start(_) | Response::Create(_) | Response::Message(_)) = reply
    {
        tending.poke();
    }
    let mut writer = stream;
    protocol::write_reply(&mut writer, &reply).map_err(Error::Protocol)
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A new id: `prefix` and ten lowercase hex digits, safe as a tmux
/// session's name: `s` for a session, `m` for a message.
pub(crate) fn new_id(prefix: char) -> String {
    let mut bytes = [0u8; 5];
    let random = File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes));
    if random.is_err() {
        bytes.copy_from_slice(&state::now_millis().to_le_bytes()[..5]);
    }
    bytes.iter().fold(String::from(prefix), |mut id, b| {
        let _ = write!(id, "{b:02x}");
        id
    })
}

/// The root of the git repository containing `path`: the nearest directory
/// at or above it holding `.git` (a directory, or a file for a worktree).
fn repo_root_of(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}
