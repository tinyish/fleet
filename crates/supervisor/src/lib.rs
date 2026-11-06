//! The supervisor: one per user, the only writer of anything tinyfleet
//! writes. It answers the CLI on a Unix socket in the state directory.

mod adapters;
mod create;
mod messages;
mod names;
mod observe;
mod overview;
mod projects;
mod remove;
#[cfg(test)]
mod scenarios;
mod sessions;
mod tending;
#[cfg(test)]
mod testing;
mod watch;
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

use fleet_core::messages::Holds;
use fleet_core::paths::{Os, state_dir};
use fleet_core::protocol::{ProtocolError, Reply, Request, Response, Status, request, response};
use fleet_core::tools::{AgentAdapters, Multiplexer, Network};
use state::State;

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
    state.write(|w| w.supervisor_started(VERSION, std::process::id()))?;
    let store = adapters::Store::new(
        state_dir,
        std::env::var(fleet_core::adapters::BASE_VAR).ok(),
    );
    let supervisor = Arc::new(Supervisor::new(state, tools, store));
    {
        let supervisor = Arc::clone(&supervisor);
        thread::spawn(move || {
            loop {
                supervisor.tick(state::now_millis());
                supervisor.tending.wait();
            }
        });
    }

    for conn in listener.incoming() {
        match conn {
            Ok(stream) => {
                let supervisor = Arc::clone(&supervisor);
                thread::spawn(move || {
                    if let Err(e) = answer(&supervisor, &stream) {
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

/// Answers the one request on `stream`.
fn answer(supervisor: &Supervisor, stream: &UnixStream) -> Result<(), Error> {
    stream
        .set_read_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io(Doing::SettingTimeout, e))?;
    stream
        .set_write_timeout(Some(TIMEOUT))
        .map_err(|e| Error::Io(Doing::SettingTimeout, e))?;
    let reply = match protocol::read_request(&mut BufReader::new(stream)) {
        Ok(request) => supervisor.ask(request)?,
        // The parser's own detail; any other failure means no reply could
        // reach the other side, so it is only logged.
        Err(ProtocolError::Malformed(error)) => Reply::Status(Status::Malformed { error }),
        Err(e) => return Err(Error::Protocol(e)),
    };
    let mut writer = stream;
    protocol::write_reply(&mut writer, &reply).map_err(Error::Protocol)
}

/// The supervisor in process: its state, the tools it drives, its kept
/// adapters, its tending loop's memory and the sessions held after a
/// paste. It answers a request, and runs a tick at the time it is given;
/// the socket and the clock are the caller's.
struct Supervisor {
    state: Mutex<State>,
    tools: Tools,
    store: adapters::Store,
    tending: tending::Tending,
    /// Only a tick reads or changes them; no request does.
    holds: Mutex<Holds>,
}

impl Supervisor {
    /// A supervisor on `state`, as it starts: nothing backing off, no
    /// session held after a paste.
    fn new(state: State, tools: Tools, store: adapters::Store) -> Supervisor {
        Supervisor {
            state: Mutex::new(state),
            tools,
            store,
            tending: tending::Tending::new(),
            holds: Mutex::default(),
        }
    }

    /// The reply to `request`. A reply that may leave work for the tending
    /// loop wakes it.
    fn ask(&self, request: Request) -> Result<Reply, Error> {
        let reply = match request {
            Request::Hello => Reply::Response(Response::Hello(response::Hello {
                version: VERSION.to_owned(),
                schema_version: Some(state::SCHEMA_VERSION),
            })),
            Request::Start(request::Start::All) => projects::all_on(self)?,
            Request::Start(request::Start::Project { project }) => {
                projects::project_on(self, &project)?
            }
            Request::Stop(request::Stop::All) => projects::all_off(self)?,
            Request::Stop(request::Stop::Project { project }) => {
                projects::project_off(self, &project)?
            }
            Request::Create(ask) => create::create(self, ask)?,
            Request::Remove(ask) => remove::remove(self, ask)?,
            Request::Spawn(ask) => sessions::spawn(self, ask)?,
            Request::Overview { directory } => overview::overview(self, &directory)?,
            Request::End(ask) => sessions::end(self, ask)?,
            Request::Message(ask) => messages::send(self, ask)?,
            Request::Events(ask) => watch::events(self, ask)?,
            Request::Wait(ask) => watch::wait(self, &ask)?,
        };
        if wakes_tending(&reply) {
            self.tending.poke();
        }
        Ok(reply)
    }

    /// One tick of the tending loop at `now_millis`.
    fn tick(&self, now_millis: i64) {
        tending::tick(self, now_millis);
    }

    /// The state, locked.
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The sessions held after a paste, locked.
    fn holds(&self) -> MutexGuard<'_, Holds> {
        self.holds.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Whether `reply` may leave work for the tending loop, so it should tend
/// now: start and create may have turned projects on or read seats, a
/// message may wait for an idle agent, and a seat whose session was ended
/// wants a new one.
fn wakes_tending(reply: &Reply) -> bool {
    matches!(
        reply,
        Reply::Response(
            Response::Start(_)
                | Response::Create(_)
                | Response::Message(_)
                | Response::End(response::End::Ended { seat: Some(_), .. })
        )
    )
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

/// `path` made canonical; for one that no longer exists, its `..` resolved
/// by text and its nearest existing ancestor made canonical, so a deleted
/// repository written `../blog` still names the project it was.
pub(crate) fn canonical(path: &Path) -> PathBuf {
    if let Ok(path) = fs::canonicalize(path) {
        return path;
    }
    let lexical = fleet_core::paths::lexical(path);
    lexical
        .ancestors()
        .find_map(|base| {
            let real = fs::canonicalize(base).ok()?;
            let rest = lexical.strip_prefix(base).ok()?;
            Some(real.join(rest))
        })
        .unwrap_or(lexical)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seats_end_wakes_the_tending_loop_and_a_spawned_sessions_does_not() {
        let ended = |seat: Option<&str>| {
            Reply::Response(Response::End(response::End::Ended {
                session: "s1".into(),
                seat: seat.map(str::to_owned),
            }))
        };
        assert!(wakes_tending(&ended(Some("scribe"))));
        assert!(!wakes_tending(&ended(None)));
    }
}
