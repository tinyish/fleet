//! Talking to the supervisor: one request, one reply, with timeouts.

use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use protocol::{ProtocolError, Reply, Request, Response, request, response};

use crate::ending::{Doubt, During, Problem, Status};

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

/// Waits until `answers` is `want`, or `WAIT` passes. Returns whether it got there.
pub(crate) fn wait_until(socket: &Path, want: bool) -> bool {
    let start = Instant::now();
    loop {
        if answers(socket) == want {
            return true;
        }
        if start.elapsed() >= WAIT {
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

/// `fleet start`'s request, after the supervisor was found or started as
/// `during` says.
pub(crate) fn start(
    socket: &Path,
    start: request::Start,
    during: During,
) -> Result<response::Start, Status> {
    ask_for(
        socket,
        &Request::Start(start),
        during,
        "start",
        |r| match r {
            Response::Start(started) => Some(started),
            _ => None,
        },
    )
}

/// `fleet stop`'s request.
pub(crate) fn stop(socket: &Path, stop: request::Stop) -> Result<response::Stop, Status> {
    ask_for(
        socket,
        &Request::Stop(stop),
        During::TurningOff,
        "stop",
        |r| match r {
            Response::Stop(stopped) => Some(stopped),
            _ => None,
        },
    )
}
