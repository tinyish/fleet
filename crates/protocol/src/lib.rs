//! What the CLI and the supervisor say to each other: one request and one
//! answer per connection, each a single line of JSON.

use std::fmt;
use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The longest line either side reads, in bytes.
pub const MAX_LINE: usize = 64 * 1024;

/// The longest socket path accepted. macOS allows 104 bytes and Linux 108;
/// this leaves room for both.
pub const MAX_SOCKET_PATH: usize = 100;

/// What the CLI asks the supervisor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// Are you there, and which version are you?
    Hello,
    /// Turn every fleet on.
    AllOn,
    /// Turn every fleet off.
    AllOff,
}

/// What the supervisor answers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Answer {
    /// The supervisor is there.
    Hello {
        /// The supervisor's version.
        version: String,
    },
    /// The request was done.
    Done {
        /// What was found or done, for a person.
        said: String,
    },
    /// The request was refused.
    Refused {
        /// Why, for a person.
        reason: String,
    },
}

/// A request or answer could not be sent or read.
#[derive(Debug)]
pub enum ProtocolError {
    /// The socket failed.
    Io(io::Error),
    /// The line was not a request or answer this version knows.
    Malformed(String),
    /// The other side closed without a line.
    Empty,
    /// The line was longer than `MAX_LINE`.
    TooLong,
    /// The socket path would be longer than `MAX_SOCKET_PATH`.
    PathTooLong(PathBuf),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProtocolError::Io(e) => write!(f, "{e}"),
            ProtocolError::Malformed(e) => write!(f, "could not understand the other side: {e}"),
            ProtocolError::Empty => f.write_str("the other side closed without answering"),
            ProtocolError::TooLong => write!(f, "a line longer than {MAX_LINE} bytes"),
            ProtocolError::PathTooLong(p) => write!(
                f,
                "the socket path {} is longer than {MAX_SOCKET_PATH} bytes; \
                 set FLEET_STATE_DIR to a shorter directory",
                p.display()
            ),
        }
    }
}

impl From<io::Error> for ProtocolError {
    fn from(e: io::Error) -> Self {
        ProtocolError::Io(e)
    }
}

/// The supervisor's socket inside `state_dir`.
///
/// # Errors
///
/// `PathTooLong` when the path would not fit in a Unix socket address.
pub fn socket_path(state_dir: &Path) -> Result<PathBuf, ProtocolError> {
    let path = state_dir.join("supervisor.sock");
    if path.as_os_str().len() > MAX_SOCKET_PATH {
        return Err(ProtocolError::PathTooLong(path));
    }
    Ok(path)
}

fn write_line(w: &mut impl Write, value: &impl Serialize) -> Result<(), ProtocolError> {
    let mut line =
        serde_json::to_vec(value).map_err(|e| ProtocolError::Malformed(e.to_string()))?;
    line.push(b'\n');
    w.write_all(&line)?;
    w.flush()?;
    Ok(())
}

fn read_line<T: for<'de> Deserialize<'de>>(r: &mut impl BufRead) -> Result<T, ProtocolError> {
    let mut line = String::new();
    let limit = u64::try_from(MAX_LINE + 1).unwrap_or(u64::MAX);
    let read = r.take(limit).read_line(&mut line)?;
    if read == 0 {
        return Err(ProtocolError::Empty);
    }
    if read > MAX_LINE {
        return Err(ProtocolError::TooLong);
    }
    serde_json::from_str(line.trim_end()).map_err(|e| ProtocolError::Malformed(e.to_string()))
}

/// Sends one request.
///
/// # Errors
///
/// When the socket fails.
pub fn write_request(w: &mut impl Write, r: &Request) -> Result<(), ProtocolError> {
    write_line(w, r)
}

/// Reads one request.
///
/// # Errors
///
/// When the socket fails or the line is empty, too long or not a request.
pub fn read_request(r: &mut impl BufRead) -> Result<Request, ProtocolError> {
    read_line(r)
}

/// Sends one answer.
///
/// # Errors
///
/// When the socket fails.
pub fn write_answer(w: &mut impl Write, a: &Answer) -> Result<(), ProtocolError> {
    write_line(w, a)
}

/// Reads one answer.
///
/// # Errors
///
/// When the socket fails or the line is empty, too long or not an answer.
pub fn read_answer(r: &mut impl BufRead) -> Result<Answer, ProtocolError> {
    read_line(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn every_request_round_trips() {
        for req in [Request::Hello, Request::AllOn, Request::AllOff] {
            let mut buf = Vec::new();
            write_request(&mut buf, &req).ok();
            assert_eq!(buf.last(), Some(&b'\n'));
            assert_eq!(read_request(&mut Cursor::new(buf)).ok(), Some(req));
        }
    }

    #[test]
    fn every_answer_round_trips() {
        let answers = [
            Answer::Hello {
                version: "0.0.0".into(),
            },
            Answer::Done {
                said: "every fleet is on".into(),
            },
            Answer::Refused {
                reason: "no".into(),
            },
        ];
        for ans in answers {
            let mut buf = Vec::new();
            write_answer(&mut buf, &ans).ok();
            assert_eq!(read_answer(&mut Cursor::new(buf)).ok(), Some(ans));
        }
    }

    #[test]
    fn the_wire_form_is_stable() {
        let mut buf = Vec::new();
        write_request(&mut buf, &Request::AllOn).ok();
        assert_eq!(buf, b"\"all_on\"\n");
    }

    #[test]
    fn garbage_empty_and_oversized_lines_are_errors() {
        assert!(read_request(&mut Cursor::new(b"nonsense\n".to_vec())).is_err());
        assert!(read_request(&mut Cursor::new(Vec::new())).is_err());
        let big = vec![b'a'; MAX_LINE + 10];
        assert!(read_request(&mut Cursor::new(big)).is_err());
    }

    #[test]
    fn socket_path_is_in_the_state_dir_and_bounded() {
        assert_eq!(
            socket_path(Path::new("/s")).ok(),
            Some(PathBuf::from("/s/supervisor.sock"))
        );
        let long = format!("/{}", "d".repeat(120));
        assert!(matches!(
            socket_path(Path::new(&long)),
            Err(ProtocolError::PathTooLong(_))
        ));
    }
}
