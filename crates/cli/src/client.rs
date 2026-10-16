//! Talking to the supervisor: one request, one answer, with timeouts.

use std::io::BufReader;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use protocol::{Answer, ProtocolError, Request};

/// How long one request may take.
pub(crate) const ONE: Duration = Duration::from_secs(2);

/// How long start and stop wait for the supervisor to come or go.
pub(crate) const WAIT: Duration = Duration::from_secs(10);

pub(crate) fn ask(socket: &Path, request: &Request) -> Result<Answer, ProtocolError> {
    let stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(ONE))?;
    stream.set_write_timeout(Some(ONE))?;
    let mut writer = &stream;
    protocol::write_request(&mut writer, request)?;
    protocol::read_answer(&mut BufReader::new(&stream))
}

pub(crate) fn answers(socket: &Path) -> bool {
    matches!(ask(socket, &Request::Hello), Ok(Answer::Hello { .. }))
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
