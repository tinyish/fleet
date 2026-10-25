//! What the CLI and the supervisor say to each other: one request and one
//! answer per connection, each a single line of JSON.

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

pub use crate::status::Status;
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
    /// For `fleet remove`.
    Remove(request::Remove),
    /// For agents' sessions: `fleet debug` now.
    Session(request::Session),
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
        /// Re-read every fleet's config, then turn every project on.
        All,
        /// Re-read every fleet's config, then turn this one project on.
        Project {
            /// The project, by its repository's root, absolute.
            project: PathBuf,
        },
    }

    /// What `fleet stop` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Stop {
        /// Turn every project off.
        All,
        /// Turn this one project off; the rest of its fleet keeps its own on
        /// or off.
        Project {
            /// The project, by its repository's root, absolute.
            project: PathBuf,
        },
    }

    /// What `fleet remove` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Remove {
        /// Remove the fleet in `directory`, or, when `projects` are named
        /// and they are not all of its projects, take them out of it.
        Fleet {
            /// The fleet directory, absolute.
            directory: PathBuf,
            /// The projects to take out, absolute; none means the fleet.
            projects: Vec<PathBuf>,
            /// What to do with the fleet's config file when the whole fleet
            /// goes.
            config: ConfigChoice,
        },
    }

    /// What is asked of agents' sessions.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Session {
        /// Launch an agent through the adapter `adapter` names, in
        /// `directory`.
        Launch {
            /// The adapter: a program by its absolute path, or
            /// `<name>@<version>`, a release of fleet-adapters
            /// (`core::adapters`).
            adapter: String,
            /// Where the agent works, absolute.
            directory: PathBuf,
            /// The model to run; the adapter's default when absent.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            model: Option<String>,
            /// The first thing the agent is told.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            prompt: Option<String>,
        },
        /// Every session, with its state.
        List,
        /// Stop a session: end its tmux session and forget it.
        Stop {
            /// The session's id.
            session: String,
        },
    }

    /// What to do with a removed fleet's config file.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ConfigChoice {
        /// Ask the person first: when the whole fleet would go and its file
        /// is there, change nothing and answer `Ask`.
        Ask,
        /// Delete it.
        Delete,
        /// Leave it where it is.
        Keep,
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
    /// What `fleet remove` did.
    Remove(response::Remove),
    /// What was done to agents' sessions.
    Session(response::Session),
}

/// The responses of each command. Always written with the module, as in
/// `response::Create`, since `request::Create` shares the name.
pub mod response {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    use crate::agent::{AdapterFault, SessionState};

    /// The supervisor is there.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Hello {
        /// The supervisor's version.
        pub version: String,
        /// The state schema the supervisor opened; absent from a supervisor
        /// older than this field, so every version's hello still reads.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub schema_version: Option<i64>,
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
        /// Whether the fleet was made from a `fleet.toml` already there,
        /// left as it was; absent from a supervisor older than this field.
        #[serde(default)]
        pub adopted: bool,
    }

    /// What `fleet start` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Start {
        /// Every config was re-read and every project of each fleet that
        /// read is on.
        AllOn(AllOn),
        /// One project is on.
        ProjectOn(ProjectOn),
    }

    /// What `fleet stop` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Stop {
        /// Every project was turned off.
        AllOff,
        /// One project is off.
        ProjectOff(ProjectOff),
    }

    /// Every config was re-read; every project of each fleet whose config
    /// read is on, and the rest stayed off.
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub struct AllOn {
        /// The projects now on.
        pub on: Vec<PathBuf>,
        /// The fleets they belong to.
        pub fleets: Vec<PathBuf>,
        /// What re-reading the configs changed.
        pub picked_up: Vec<PickedUp>,
        /// The fleets whose config did not read, so their projects are off.
        pub stayed_off: Vec<StayedOff>,
        /// The fleets whose directory no longer exists, so they were
        /// forgotten; absent from a supervisor older than this field.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub forgotten: Vec<PathBuf>,
    }

    /// One project is on.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ProjectOn {
        /// The project.
        pub project: PathBuf,
        /// What re-reading the configs changed.
        pub picked_up: Vec<PickedUp>,
        /// The fleets whose directory no longer exists, so they were
        /// forgotten; absent from a supervisor older than this field.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub forgotten: Vec<PathBuf>,
    }

    /// One project is off; the rest of its fleet keeps its own on or off.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct ProjectOff {
        /// The project.
        pub project: PathBuf,
        /// The other projects of its fleet, left as they were.
        pub others: Vec<PathBuf>,
    }

    /// What re-reading one fleet's config changed in its projects.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct PickedUp {
        /// The fleet.
        pub fleet: PathBuf,
        /// Projects its config now names.
        pub added: Vec<PathBuf>,
        /// Projects its config no longer names.
        pub removed: Vec<PathBuf>,
    }

    /// What `fleet remove` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Remove {
        /// The fleet is gone, with what became of its config file.
        Removed {
            /// The fleet directory.
            directory: PathBuf,
            /// The projects it served.
            projects: Vec<PathBuf>,
            /// What became of its config file.
            config: ConfigFate,
        },
        /// The wingman fleet stays; these projects are no longer its.
        ProjectsTaken {
            /// The fleet directory.
            directory: PathBuf,
            /// The projects taken out.
            taken: Vec<PathBuf>,
            /// The projects it still serves.
            remaining: Vec<PathBuf>,
        },
        /// The whole fleet would go and its config file is there, and the
        /// request said to ask first: nothing changed. Ask the person, then
        /// send the request again with their answer.
        Ask {
            /// The fleet directory.
            directory: PathBuf,
        },
        /// No fleet is registered in `directory`; nothing changed.
        NothingHere {
            /// The directory remove was run in.
            directory: PathBuf,
        },
    }

    /// What became of a removed fleet's config file.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum ConfigFate {
        /// It was deleted.
        Deleted,
        /// It was left where it is.
        Kept,
        /// There was none.
        Missing,
    }

    /// What was done to agents' sessions.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Session {
        /// A session was launched.
        Launched {
            /// Its id.
            session: String,
        },
        /// Every session, with its state.
        Listed {
            /// The sessions, oldest first.
            sessions: Vec<Listed>,
            /// The adapters that failed to read their sessions, which are
            /// unknown.
            #[serde(default, skip_serializing_if = "Vec::is_empty")]
            faults: Vec<Fault>,
        },
        /// A session was stopped.
        Stopped {
            /// Its id.
            session: String,
        },
    }

    /// One session and its state.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Listed {
        /// Its id.
        pub session: String,
        /// The adapter program that launched it.
        pub adapter: PathBuf,
        /// Where its agent works.
        pub directory: PathBuf,
        /// Its state now.
        pub state: SessionState,
        /// When it was launched.
        pub launch_time_millis: i64,
    }

    /// An adapter that failed to read its sessions.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Fault {
        /// The adapter program.
        pub adapter: PathBuf,
        /// How it failed.
        pub fault: AdapterFault,
    }

    /// A fleet whose config did not read, and why.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct StayedOff {
        /// The fleet.
        pub fleet: PathBuf,
        /// Why its config did not read.
        pub why: super::Status,
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
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(ProtocolErrorKind), derive(strum::EnumIter))
)]
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
    use crate::agent::{AdapterFault, Reason, SessionState, Verb};
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
            Request::Start(request::Start::Project {
                project: PathBuf::from("/c/app"),
            }),
            Request::Stop(request::Stop::Project {
                project: PathBuf::from("/c/app"),
            }),
            create,
            Request::Session(request::Session::Launch {
                adapter: "claude-code@2.1.285".into(),
                directory: PathBuf::from("/c/app"),
                model: Some("sonnet".into()),
                prompt: Some("hi; there".into()),
            }),
            Request::Session(request::Session::List),
            Request::Session(request::Session::Stop {
                session: "s1".into(),
            }),
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
                schema_version: Some(1),
            })),
            Reply::Response(Response::Start(response::Start::AllOn(response::AllOn {
                on: vec![PathBuf::from("/c/app")],
                fleets: vec![PathBuf::from("/c/app")],
                picked_up: vec![response::PickedUp {
                    fleet: PathBuf::from("/f/w"),
                    added: vec![PathBuf::from("/c/web")],
                    removed: vec![],
                }],
                stayed_off: vec![response::StayedOff {
                    fleet: PathBuf::from("/f/w"),
                    why: Status::BadConfig {
                        path: PathBuf::from("/f/w/fleet.toml"),
                        line: Some(1),
                        column: None,
                        error: "expected `]`".into(),
                    },
                }],
                forgotten: vec![PathBuf::from("/f/old")],
            }))),
            Reply::Response(Response::Start(response::Start::ProjectOn(
                response::ProjectOn {
                    project: PathBuf::from("/c/app"),
                    picked_up: vec![],
                    forgotten: vec![],
                },
            ))),
            Reply::Response(Response::Stop(response::Stop::ProjectOff(
                response::ProjectOff {
                    project: PathBuf::from("/c/app"),
                    others: vec![PathBuf::from("/c/api")],
                },
            ))),
            Reply::Status(Status::CannotTurnOn {
                project: PathBuf::from("/c/api"),
                fleet: PathBuf::from("/f/w"),
                why: Box::new(Status::AlsoClaimed {
                    project: PathBuf::from("/c/api"),
                    fleet: PathBuf::from("/f/a"),
                }),
            }),
            Reply::Response(Response::Stop(response::Stop::AllOff)),
            Reply::Response(Response::Create(response::Create {
                directory: PathBuf::from("/f/work"),
                projects: vec![PathBuf::from("/c/app")],
                created: true,
                adopted: true,
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
    fn every_session_reply_round_trips() {
        let replies = [
            Reply::Response(Response::Session(response::Session::Launched {
                session: "s1".into(),
            })),
            Reply::Response(Response::Session(response::Session::Listed {
                sessions: vec![response::Listed {
                    session: "s1".into(),
                    adapter: PathBuf::from("/a/claude"),
                    directory: PathBuf::from("/c/app"),
                    state: SessionState::NeedsYou {
                        reason: Some(Reason::Permission),
                    },
                    launch_time_millis: 5,
                }],
                faults: vec![response::Fault {
                    adapter: PathBuf::from("/a/codex"),
                    fault: AdapterFault::TimedOut {
                        after_millis: 20_000,
                    },
                }],
            })),
            Reply::Response(Response::Session(response::Session::Stopped {
                session: "s1".into(),
            })),
            Reply::Status(Status::AdapterFailed {
                adapter: PathBuf::from("/a/claude"),
                verb: Verb::Launch,
                fault: AdapterFault::Exited {
                    code: Some(1),
                    said: "claude: not found".into(),
                },
            }),
            Reply::Status(Status::MultiplexerCouldNotRun {
                error: "No such file or directory (os error 2)".into(),
            }),
            Reply::Status(Status::MultiplexerFailed {
                command: "tmux new-session".into(),
                said: "no".into(),
            }),
            Reply::Status(Status::NoSuchSession {
                session: "s9".into(),
            }),
        ];
        for reply in replies {
            let mut buf = Vec::new();
            write_reply(&mut buf, &reply).ok();
            assert_eq!(read_reply(&mut Cursor::new(buf)).ok(), Some(reply));
        }
    }

    #[test]
    fn the_session_wire_nests_by_command() {
        assert_eq!(
            read_request(&mut Cursor::new(b"{\"session\":\"list\"}\n".to_vec())).ok(),
            Some(Request::Session(request::Session::List))
        );
        let mut buf = Vec::new();
        let listed = response::Session::Listed {
            sessions: vec![],
            faults: vec![],
        };
        write_reply(&mut buf, &Reply::Response(Response::Session(listed))).ok();
        assert_eq!(
            buf,
            b"{\"response\":{\"session\":{\"listed\":{\"sessions\":[]}}}}\n"
        );
    }

    #[test]
    fn a_hello_from_an_older_supervisor_still_reads() {
        let old = b"{\"response\":{\"hello\":{\"version\":\"0.0.0\"}}}\n".to_vec();
        assert_eq!(
            read_reply(&mut Cursor::new(old)).ok(),
            Some(Reply::Response(Response::Hello(response::Hello {
                version: "0.0.0".into(),
                schema_version: None,
            })))
        );
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

    #[test]
    fn a_remove_and_its_answers_round_trip() {
        let request = Request::Remove(request::Remove::Fleet {
            directory: "/w".into(),
            projects: vec!["/c/api".into()],
            config: request::ConfigChoice::Ask,
        });
        let line = serde_json::to_string(&request).unwrap_or_default();
        assert_eq!(serde_json::from_str::<Request>(&line).ok(), Some(request));
        for answer in [
            response::Remove::Removed {
                directory: "/w".into(),
                projects: vec!["/c/api".into()],
                config: response::ConfigFate::Kept,
            },
            response::Remove::ProjectsTaken {
                directory: "/w".into(),
                taken: vec!["/c/blog".into()],
                remaining: vec!["/c/api".into()],
            },
            response::Remove::Ask {
                directory: "/w".into(),
            },
            response::Remove::NothingHere {
                directory: "/w".into(),
            },
        ] {
            let reply = Reply::Response(Response::Remove(answer));
            let line = serde_json::to_string(&reply).unwrap_or_default();
            assert_eq!(serde_json::from_str::<Reply>(&line).ok(), Some(reply));
        }
    }

    #[test]
    fn a_start_answer_without_forgotten_still_reads() {
        let line = r#"{"response":{"start":{"all_on":{"on":[],"fleets":[],"picked_up":[],"stayed_off":[]}}}}"#;
        let reply = serde_json::from_str::<Reply>(line).ok();
        assert!(
            matches!(&reply, Some(Reply::Response(Response::Start(response::Start::AllOn(all)))) if all.forgotten.is_empty()),
            "{reply:?}"
        );
    }
}
