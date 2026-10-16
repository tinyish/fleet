//! What the CLI and the supervisor say to each other: one request and one
//! answer per connection, each a single line of JSON.

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

pub use fleet_core::status::Status;
use serde::{Deserialize, Serialize};

/// The longest line either side reads, in bytes.
pub const MAX_LINE: usize = 64 * 1024;

/// The longest socket path accepted. macOS allows 104 bytes and Linux 108;
/// this leaves room for both.
pub const MAX_SOCKET_PATH: usize = 100;

/// What the CLI asks the supervisor, grouped by the command that asks it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// Are you there, and which version are you?
    Hello,
    /// For `fleet create`.
    Create(request::Create),
    /// For `fleet start`.
    Start(request::Start),
    /// For `fleet stop`.
    Stop(request::Stop),
}

/// The requests of each command. Always written with the module, as in
/// `request::Create`, since `response::Create` shares the name.
pub mod request {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    /// What `fleet create` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Create {
        /// Create a fleet in `directory`. No `projects` means a default
        /// fleet, its directory its one project; otherwise a wingman fleet.
        Fleet {
            /// The fleet directory, absolute.
            directory: PathBuf,
            /// The repositories a wingman fleet serves, absolute.
            projects: Vec<PathBuf>,
        },
    }

    /// What `fleet start` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Start {
        /// Turn every fleet on.
        All,
    }

    /// What `fleet stop` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Stop {
        /// Turn every fleet off.
        All,
    }
}

/// What a request did, when the supervisor did it, grouped by the command
/// that asked. Facts only, never sentences: the CLI writes every word a person
/// reads (core.md invariant 9). The supervisor's whole reply is
/// a [`Reply`]: this, or a [`Status`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    /// The supervisor is there.
    Hello(response::Hello),
    /// What `fleet create` did.
    Create(response::Create),
    /// What `fleet start` did.
    Start(response::Start),
    /// What `fleet stop` did.
    Stop(response::Stop),
}

/// The responses of each command. Always written with the module, as in
/// `response::Create`, since `request::Create` shares the name.
pub mod response {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    /// The supervisor is there.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Hello {
        /// The supervisor's version.
        pub version: String,
    }

    /// What `fleet create` did: the fleet in `directory`, serving
    /// `projects` (its directory alone for a default fleet, the repositories
    /// it serves for a wingman fleet).
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Create {
        /// The fleet directory.
        pub directory: PathBuf,
        /// The projects it serves.
        pub projects: Vec<PathBuf>,
        /// Whether create made the fleet; `false` when the same fleet was
        /// already there and nothing changed.
        pub created: bool,
    }

    /// What `fleet start` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Start {
        /// Every fleet was turned on.
        AllOn,
    }

    /// What `fleet stop` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Stop {
        /// Every fleet was turned off.
        AllOff,
    }
}

/// The supervisor's whole reply to one request: what it did, or why not.
/// On the wire, `{"response": …}` or `{"status": …}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    /// The request was done; this is what happened.
    Response(Response),
    /// The request was refused; this is why.
    Status(Status),
}

/// The socket path would not fit in a Unix socket address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SocketPathTooLong(pub PathBuf);

/// The conversation with the other side failed: the socket, a line that is
/// garbled, missing or too long. Never a reply.
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
/// When the path would not fit in a Unix socket address.
pub fn socket_path(state_dir: &Path) -> Result<PathBuf, SocketPathTooLong> {
    let path = state_dir.join("supervisor.sock");
    if path.as_os_str().len() > MAX_SOCKET_PATH {
        return Err(SocketPathTooLong(path));
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

/// Sends one reply.
///
/// # Errors
///
/// When the socket fails.
pub fn write_reply(w: &mut impl Write, reply: &Reply) -> Result<(), ProtocolError> {
    write_line(w, reply)
}

/// Reads one reply.
///
/// # Errors
///
/// When the socket fails or the line is empty, too long or not a reply.
pub fn read_reply(r: &mut impl BufRead) -> Result<Reply, ProtocolError> {
    read_line(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn every_request_round_trips() {
        let create = Request::Create(request::Create::Fleet {
            directory: PathBuf::from("/f/my work"),
            projects: vec![PathBuf::from("/c/app")],
        });
        let requests = [
            Request::Hello,
            Request::Start(request::Start::All),
            Request::Stop(request::Stop::All),
            create,
        ];
        for req in requests {
            let mut buf = Vec::new();
            write_request(&mut buf, &req).ok();
            assert_eq!(buf.last(), Some(&b'\n'));
            assert_eq!(read_request(&mut Cursor::new(buf)).ok(), Some(req));
        }
    }

    #[test]
    fn every_reply_round_trips() {
        let replies = [
            Reply::Response(Response::Hello(response::Hello {
                version: "0.0.0".into(),
            })),
            Reply::Response(Response::Start(response::Start::AllOn)),
            Reply::Response(Response::Stop(response::Stop::AllOff)),
            Reply::Response(Response::Create(response::Create {
                directory: PathBuf::from("/f/work"),
                projects: vec![PathBuf::from("/c/app")],
                created: true,
            })),
            Reply::Status(Status::AlreadyServed {
                project: PathBuf::from("/c/app"),
                fleet: PathBuf::from("/f/work"),
            }),
            Reply::Status(Status::Missing {
                path: PathBuf::from("/x"),
            }),
        ];
        for reply in replies {
            let mut buf = Vec::new();
            write_reply(&mut buf, &reply).ok();
            assert_eq!(read_reply(&mut Cursor::new(buf)).ok(), Some(reply));
        }
    }

    #[test]
    fn the_wire_nests_by_command_and_never_carries_a_sentence() {
        let mut buf = Vec::new();
        write_request(&mut buf, &Request::Start(request::Start::All)).ok();
        assert_eq!(buf, b"{\"start\":\"all\"}\n");
        let mut buf = Vec::new();
        write_reply(
            &mut buf,
            &Reply::Response(Response::Stop(response::Stop::AllOff)),
        )
        .ok();
        assert_eq!(buf, b"{\"response\":{\"stop\":\"all_off\"}}\n");
        let mut buf = Vec::new();
        let missing = Status::Missing {
            path: PathBuf::from("/x"),
        };
        write_reply(&mut buf, &Reply::Status(missing)).ok();
        assert_eq!(buf, b"{\"status\":{\"missing\":{\"path\":\"/x\"}}}\n");
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
            Err(SocketPathTooLong(_))
        ));
    }
}
