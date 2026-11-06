//! Talking to the supervisor: one request, one reply, with timeouts.

use fleet_core::protocol;
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use fleet_core::protocol::request;
use fleet_core::protocol::{ProtocolError, Reply, Request, Response, response};
use fleet_core::tools::{ADAPTER_TIMEOUT, DOWNLOAD_TIMEOUT};

use crate::protocol::{Doubt, During, Problem, Startup, Status};

/// How long one request may take.
pub(crate) const ONE: Duration = Duration::from_secs(2);

/// How long start and stop wait for the supervisor to come or go.
pub(crate) const WAIT: Duration = Duration::from_secs(10);

/// The supervisor's socket in `state_dir`.
pub(crate) fn socket(state_dir: &Path) -> Result<PathBuf, Status> {
    protocol::socket_path(state_dir).map_err(|e| Status::Cannot(Problem::SocketPathTooLong(e.0)))
}

/// Says `request` to the supervisor, waiting at most `within` for each of
/// the question and the answer: its reply, or why the conversation failed.
fn exchange(socket: &Path, request: &Request, within: Duration) -> Result<Reply, ProtocolError> {
    let stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(within))?;
    stream.set_write_timeout(Some(within))?;
    let mut writer = &stream;
    protocol::write_request(&mut writer, request)?;
    protocol::read_reply(&mut BufReader::new(&stream))
}

pub(crate) fn answers(socket: &Path) -> bool {
    matches!(
        exchange(socket, &Request::Hello, ONE),
        Ok(Reply::Response(Response::Hello(_)))
    )
}

/// What asking the supervisor for hello came to.
#[derive(Debug)]
pub(crate) enum Hello {
    /// It answered.
    Answered(response::Hello),
    /// Nothing listens on the socket.
    NotListening,
    /// It did not answer within `ONE`.
    TimedOut,
    /// Talking to it failed some other way.
    Failed(ProtocolError),
    /// It answered something other than hello.
    Odd,
}

/// Asks the supervisor for hello, and says how that went.
pub(crate) fn hello(socket: &Path) -> Hello {
    use std::io::ErrorKind;
    match exchange(socket, &Request::Hello, ONE) {
        Ok(Reply::Response(Response::Hello(hello))) => Hello::Answered(hello),
        Ok(_) => Hello::Odd,
        Err(ProtocolError::Io(e))
            if matches!(e.kind(), ErrorKind::ConnectionRefused | ErrorKind::NotFound) =>
        {
            Hello::NotListening
        }
        Err(ProtocolError::Io(e))
            if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
        {
            Hello::TimedOut
        }
        Err(e) => Hello::Failed(e),
    }
}

/// Waits until `answers` is `want`, or `within` passes. Returns whether it got there.
pub(crate) fn wait_until(socket: &Path, want: bool, within: Duration) -> bool {
    let start = Instant::now();
    loop {
        if answers(socket) == want {
            return true;
        }
        if start.elapsed() >= within {
            return false;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

/// The supervisor, answering on its socket.
pub(crate) struct Supervisor {
    /// Its socket.
    pub(crate) socket: PathBuf,
    /// Whether it was running or this command started it.
    pub(crate) startup: Startup,
}

impl Supervisor {
    /// Asks it `request`, waiting as long as that kind of request may take,
    /// and keeps what `pick` takes of its response: a response `pick` does
    /// not take is an odd reply, the supervisor's no is a refusal, and a
    /// failed conversation is something the CLI cannot tell.
    pub(crate) fn ask<T>(
        &self,
        request: &Request,
        pick: impl FnOnce(Response) -> Option<T>,
    ) -> Result<T, Status> {
        let (within, during) = asking(request, self.startup);
        match exchange(&self.socket, request, within) {
            Ok(Reply::Response(answer)) => {
                pick(answer).ok_or(Status::CouldNotTell(Doubt::OddReply { during }))
            }
            Ok(Reply::Status(status)) => Err(Status::Refused(status)),
            Err(error) => Err(Status::CouldNotTell(Doubt::Socket { during, error })),
        }
    }
}

/// The supervisor, if it answers on its socket in `state_dir`; it is never
/// started here.
pub(crate) fn running(state_dir: &Path) -> Result<Supervisor, Status> {
    let socket = socket(state_dir)?;
    if !answers(&socket) {
        return Err(Status::NotRunning);
    }
    let startup = Startup::WasRunning;
    Ok(Supervisor { socket, startup })
}

/// The supervisor, and the current directory to build a request from, for
/// a command asked from here. It never starts the supervisor: starting it
/// would turn every project on.
pub(crate) fn from_here(state_dir: &Path) -> Result<(Supervisor, PathBuf), Status> {
    let supervisor = running(state_dir)?;
    let directory =
        std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    Ok((supervisor, directory))
}

/// How long `request` may take, and what it asks for, said when talking
/// fails; `startup` is how the supervisor was found.
fn asking(request: &Request, startup: Startup) -> (Duration, During) {
    match request {
        Request::Hello => (ONE, During::Greeting),
        Request::Create(_) => (ONE, During::Creating),
        Request::Start(request::Start::All) => (ONE, During::TurningOn(startup)),
        Request::Start(request::Start::Project { .. }) => (ONE, During::TurningOnProject(startup)),
        Request::Stop(request::Stop::All) => (ONE, During::TurningOff),
        Request::Stop(request::Stop::Project { .. }) => (ONE, During::TurningOffProject),
        Request::Remove(_) => (ONE, During::Removing),
        // The supervisor asks each kind of adapter in turn to read its
        // sessions, and each may take `ADAPTER_TIMEOUT` before it is a
        // fault: two hung kinds, one after the other, plus a margin (60 s),
        // so their faults reach status.
        Request::Overview { .. } => (
            Duration::from_secs(2 * ADAPTER_TIMEOUT.as_secs() + 20),
            During::Looking,
        ),
        Request::Message(_) => (WAIT, During::Messaging),
        // Longer than a first fetch of an adapter at its slowest, two
        // downloads (its manifest, then its build), the build's protocol
        // check and then its launch, plus a margin (170 s), so a slow fetch's
        // or adapter's fault reaches spawn, and a spawn that succeeds tells
        // its caller the session's id.
        Request::Spawn(_) => (
            Duration::from_secs(
                2 * DOWNLOAD_TIMEOUT.as_secs() + 2 * ADAPTER_TIMEOUT.as_secs() + 10,
            ),
            During::Spawning,
        ),
        Request::End(_) => (WAIT, During::Ending),
        Request::Events(request::Events::Follow { hold_millis, .. }) => {
            (held(*hold_millis), During::Following)
        }
        Request::Wait(
            request::Wait::For { hold_millis, .. } | request::Wait::On { hold_millis, .. },
        ) => (held(*hold_millis), During::Waiting),
    }
}

/// How long a poll the supervisor may hold `hold_millis` is waited for: 10 s
/// more.
fn held(hold_millis: u64) -> Duration {
    Duration::from_millis(hold_millis).saturating_add(Duration::from_secs(10))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn within(request: &Request) -> Duration {
        asking(request, Startup::WasRunning).0
    }

    #[test]
    fn spawn_outwaits_a_first_fetch_and_launch() {
        let spawn = within(&Request::Spawn(request::Spawn::Session {
            directory: "/c/app".into(),
            role: "worker".into(),
            working_directory: None,
        }));
        assert!(
            spawn > 2 * DOWNLOAD_TIMEOUT + 2 * ADAPTER_TIMEOUT,
            "{spawn:?}"
        );
    }

    #[test]
    fn status_outwaits_two_hung_adapter_kinds() {
        let status = within(&Request::Overview {
            directory: "/c/app".into(),
        });
        assert!(status > 2 * ADAPTER_TIMEOUT, "{status:?}");
    }

    #[test]
    fn from_here_never_starts_the_supervisor() {
        let dir = std::env::temp_dir().join(format!("fleet-here-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(from_here(&dir), Err(Status::NotRunning)));
        let socket = protocol::socket_path(&dir).ok();
        assert!(socket.is_some_and(|s| !s.exists()));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn end() -> Request {
        Request::End(request::End::Session {
            directory: "/c/app".into(),
            name: "scribe".into(),
        })
    }

    /// End's pick.
    fn ended(answer: Response) -> Option<response::End> {
        match answer {
            Response::End(ended) => Some(ended),
            _ => None,
        }
    }

    #[test]
    fn ask_keeps_what_its_pick_takes() {
        let dir = std::env::temp_dir().join(format!("fleet-ask-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let Ok(socket) = protocol::socket_path(&dir) else {
            unreachable!()
        };
        let Ok(listener) = UnixListener::bind(&socket) else {
            unreachable!()
        };
        let ended_one = response::End::AlreadyEnded {
            session: "s1".into(),
        };
        let replies = vec![
            Reply::Response(Response::End(ended_one.clone())),
            Reply::Response(Response::Stop(response::Stop::AllOff)),
            Reply::Status(protocol::Status::Message(
                fleet_core::status::Message::EmptyText,
            )),
            Reply::Response(Response::Stop(response::Stop::AllOff)),
        ];
        let fake = thread::spawn(move || {
            for reply in replies {
                let Ok((stream, _)) = listener.accept() else {
                    return;
                };
                let _ = protocol::read_request(&mut BufReader::new(&stream));
                let _ = protocol::write_reply(&mut &stream, &reply);
            }
        });
        let found = Supervisor {
            socket,
            startup: Startup::Started,
        };
        assert_eq!(found.ask(&end(), ended).ok(), Some(ended_one));
        assert!(matches!(
            found.ask(&end(), ended),
            Err(Status::CouldNotTell(Doubt::OddReply {
                during: During::Ending
            }))
        ));
        assert!(matches!(found.ask(&end(), ended), Err(Status::Refused(_))));
        let start = Request::Start(request::Start::All);
        assert!(matches!(
            found.ask(&start, ended),
            Err(Status::CouldNotTell(Doubt::OddReply {
                during: During::TurningOn(Startup::Started)
            }))
        ));
        let _ = fake.join();
        assert!(matches!(
            found.ask(&end(), ended),
            Err(Status::CouldNotTell(Doubt::Socket {
                during: During::Ending,
                ..
            }))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
