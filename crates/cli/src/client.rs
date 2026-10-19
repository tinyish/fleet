//! Talking to the supervisor: one request, one reply, with timeouts.

use fleet_core::protocol;
use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use fleet_core::protocol::{ProtocolError, Reply, Request, Response, request, response};

use crate::protocol::{Doubt, During, Problem, Startup, Status};

/// How long one request may take.
pub(crate) const ONE: Duration = Duration::from_secs(2);

/// How long start and stop wait for the supervisor to come or go.
pub(crate) const WAIT: Duration = Duration::from_secs(10);

/// The supervisor's socket in `state_dir`.
pub(crate) fn socket(state_dir: &Path) -> Result<PathBuf, Status> {
    protocol::socket_path(state_dir).map_err(|e| Status::Cannot(Problem::SocketPathTooLong(e.0)))
}

/// Asks the supervisor one thing: its reply, or why the conversation failed.
fn ask(socket: &Path, request: &Request) -> Result<Reply, ProtocolError> {
    let stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(ONE))?;
    stream.set_write_timeout(Some(ONE))?;
    let mut writer = &stream;
    protocol::write_request(&mut writer, request)?;
    protocol::read_reply(&mut BufReader::new(&stream))
}

pub(crate) fn answers(socket: &Path) -> bool {
    matches!(
        ask(socket, &Request::Hello),
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
    match ask(socket, &Request::Hello) {
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

/// Asks for `request` and keeps only the reply of the kind `pick` takes: a
/// reply of another kind is an odd reply, the supervisor's no is a refusal,
/// and a failed conversation is something the CLI cannot tell.
fn ask_for<T>(
    socket: &Path,
    request: &Request,
    during: During,
    to: &'static str,
    pick: impl FnOnce(Response) -> Option<T>,
) -> Result<T, Status> {
    match ask(socket, request) {
        Ok(Reply::Response(response)) => {
            pick(response).ok_or(Status::CouldNotTell(Doubt::OddReply { to }))
        }
        Ok(Reply::Status(status)) => Err(Status::Refused(status)),
        Err(error) => Err(Status::CouldNotTell(Doubt::Socket { during, error })),
    }
}

/// `fleet create`'s request.
pub(crate) fn create(socket: &Path, create: request::Create) -> Result<response::Create, Status> {
    ask_for(
        socket,
        &Request::Create(create),
        During::Creating,
        "create",
        |r| match r {
            Response::Create(created) => Some(created),
            _ => None,
        },
    )
}

/// `fleet remove`'s request.
pub(crate) fn remove(socket: &Path, remove: request::Remove) -> Result<response::Remove, Status> {
    ask_for(
        socket,
        &Request::Remove(remove),
        During::Removing,
        "remove",
        |r| match r {
            Response::Remove(removed) => Some(removed),
            _ => None,
        },
    )
}

/// `fleet start`'s request for every project, after the supervisor was
/// found or started as `startup` says.
pub(crate) fn start_all(socket: &Path, startup: Startup) -> Result<response::AllOn, Status> {
    ask_for(
        socket,
        &Request::Start(request::Start::All),
        During::TurningOn(startup),
        "start",
        |r| match r {
            Response::Start(response::Start::AllOn(all)) => Some(all),
            _ => None,
        },
    )
}

/// `fleet start --project`'s request.
pub(crate) fn start_project(
    socket: &Path,
    project: PathBuf,
    startup: Startup,
) -> Result<response::ProjectOn, Status> {
    ask_for(
        socket,
        &Request::Start(request::Start::Project { project }),
        During::TurningOnProject(startup),
        "start --project",
        |r| match r {
            Response::Start(response::Start::ProjectOn(on)) => Some(on),
            _ => None,
        },
    )
}

/// `fleet stop`'s request for every project.
pub(crate) fn stop_all(socket: &Path) -> Result<(), Status> {
    ask_for(
        socket,
        &Request::Stop(request::Stop::All),
        During::TurningOff,
        "stop",
        |r| match r {
            Response::Stop(response::Stop::AllOff) => Some(()),
            _ => None,
        },
    )
}

/// `fleet stop --project`'s request.
pub(crate) fn stop_project(
    socket: &Path,
    project: PathBuf,
) -> Result<response::ProjectOff, Status> {
    ask_for(
        socket,
        &Request::Stop(request::Stop::Project { project }),
        During::TurningOffProject,
        "stop --project",
        |r| match r {
            Response::Stop(response::Stop::ProjectOff(off)) => Some(off),
            _ => None,
        },
    )
}
