//! What the CLI and the supervisor say to each other: one request and one
//! answer per connection, each a single line of JSON.

use std::io::{self, BufRead, Read, Write};
use std::path::{Path, PathBuf};

pub use crate::status::Status;

mod observe;
pub use observe::{BATCH_BYTES, Event, MAX_HOLD_MILLIS, Waited};
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
    /// For `fleet status`: the seats of the fleet holding `directory`, or
    /// of every fleet when none holds it.
    Overview {
        /// The CLI's current directory, absolute.
        directory: PathBuf,
    },
    /// For `fleet message`.
    Message(request::Message),
    /// For `fleet spawn`.
    Spawn(request::Spawn),
    /// For `fleet end`.
    End(request::End),
    /// For `fleet events`: one poll.
    Events(request::Events),
    /// For `fleet wait`: one poll.
    Wait(request::Wait),
}

/// The requests of each command. Always written with the module, as in
/// `request::Create`, since `response::Create` shares the name.
pub mod request {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    pub use super::observe::request::{Events, Wait};

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

    /// What `fleet spawn` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Spawn {
        /// Spawn a session from `role`, a role of the fleet holding
        /// `directory`.
        Session {
            /// The CLI's current directory, absolute.
            directory: PathBuf,
            /// The role's label.
            role: String,
            /// Where the agent works, absolute: one of the fleet's projects
            /// by its root; the role's directory when absent.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            working_directory: Option<PathBuf>,
        },
    }

    /// What `fleet end` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum End {
        /// End `name`: a seat of the fleet holding `directory`, which ends
        /// its live session, else a session by its id.
        Session {
            /// The CLI's current directory, absolute.
            directory: PathBuf,
            /// A seat's name or a session's id.
            name: String,
        },
    }

    /// What `fleet message` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Message {
        /// Queue `text` for `to`: a seat of the fleet holding `directory`,
        /// else a session by its id.
        Send {
            /// The CLI's current directory, absolute.
            directory: PathBuf,
            /// A seat's name or a session's id.
            to: String,
            /// What to type in.
            text: String,
            /// Who sends it.
            sender: crate::messages::Sender,
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
    /// What `fleet status` shows.
    Overview(response::Overview),
    /// What `fleet message` did.
    Message(response::Message),
    /// What `fleet spawn` did.
    Spawn(response::Spawn),
    /// What `fleet end` did.
    End(response::End),
    /// What one poll of `fleet events` found.
    Events(response::Events),
    /// What one poll of `fleet wait` came to.
    Wait(response::Wait),
}

/// The responses of each command. Always written with the module, as in
/// `response::Create`, since `request::Create` shares the name.
pub mod response {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    pub use super::observe::response::{Events, Wait};

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

    /// What `fleet spawn` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Spawn {
        /// A session runs.
        Spawned {
            /// Its id.
            session: String,
            /// The fleet whose role it was spawned from.
            fleet: PathBuf,
            /// That role's label.
            role: String,
            /// Where its agent works.
            directory: PathBuf,
        },
    }

    /// What `fleet end` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum End {
        /// A session was ended and forgotten.
        Ended {
            /// Its id.
            session: String,
            /// The seat whose session it was, when that seat is still in
            /// its fleet's config, so it comes back fresh while its fleet
            /// is on; none for any other session, an orphan's included.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            seat: Option<String>,
        },
    }

    mod overview;
    pub use overview::{
        Down, Fault, FleetOverview, Listed, Orphan, Overview, SeatOverview, Spawned,
    };

    /// What `fleet message` did.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Message {
        /// The message is held; it is typed in once its agent is idle.
        Queued {
            /// Its id.
            message: String,
            /// Who it is for.
            to: crate::messages::Recipient,
        },
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
    use crate::agent::{AdapterFault, Reason, SessionState, StateName, Verb};
    use crate::messages::Dropped;
    use crate::record::Step;
    use crate::status;
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
            Request::Spawn(request::Spawn::Session {
                directory: PathBuf::from("/c/wing"),
                role: "worker".into(),
                working_directory: Some(PathBuf::from("/c/app")),
            }),
            Request::Spawn(request::Spawn::Session {
                directory: PathBuf::from("/c/wing"),
                role: "worker".into(),
                working_directory: None,
            }),
            Request::Overview {
                directory: PathBuf::from("/c/app/src"),
            },
            Request::End(request::End::Session {
                directory: PathBuf::from("/c/app"),
                name: "scribe".into(),
            }),
            Request::Message(request::Message::Send {
                directory: PathBuf::from("/c/app"),
                to: "scribe".into(),
                text: "write the notes;\n#now".into(),
                sender: crate::messages::Sender::Session {
                    session: "s1".into(),
                },
            }),
            Request::Events(request::Events::Follow {
                directory: PathBuf::from("/c/app"),
                from: Some(1042),
                hold_millis: 25_000,
            }),
            Request::Events(request::Events::Follow {
                directory: PathBuf::from("/c/app"),
                from: None,
                hold_millis: 0,
            }),
            Request::Wait(request::Wait::For {
                directory: PathBuf::from("/c/app"),
                name: "scribe".into(),
                states: vec![StateName::Idle, StateName::NeedsYou],
                hold_millis: 25_000,
            }),
            Request::Wait(request::Wait::On {
                on: Waited::Seat {
                    fleet: PathBuf::from("/c/app"),
                    seat: "scribe".into(),
                },
                states: vec![StateName::Gone],
                from: 7,
                hold_millis: 3_000,
            }),
            Request::Wait(request::Wait::On {
                on: Waited::Message {
                    message: "m1".into(),
                },
                states: vec![],
                from: 7,
                hold_millis: 3_000,
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
    fn an_overview_from_before_waiting_still_reads() {
        let line = r#"{"session":"s1","adapter":"/a","directory":"/c","state":"idle","launch_time_millis":0}"#;
        let listed: Option<response::Listed> = serde_json::from_str(line).ok();
        assert_eq!(listed.map(|l| l.waiting), Some(0));
    }

    #[test]
    fn every_reply_round_trips() {
        let replies = [
            Reply::Response(Response::Message(response::Message::Queued {
                message: "m0123456789".into(),
                to: crate::messages::Recipient::Seat {
                    fleet: PathBuf::from("/c/app"),
                    seat: "scribe".into(),
                },
            })),
            Reply::Status(Status::Message(status::Message::SessionGone {
                session: "s1".into(),
            })),
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
                    why: Status::File(status::File::BadConfig {
                        path: PathBuf::from("/f/w/fleet.toml"),
                        line: Some(1),
                        column: None,
                        error: "expected `]`".into(),
                    }),
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
            Reply::Status(Status::Project(status::Project::CannotTurnOn {
                project: PathBuf::from("/c/api"),
                fleet: PathBuf::from("/f/w"),
                why: Box::new(Status::Fleet(status::Fleet::AlsoClaimed {
                    project: PathBuf::from("/c/api"),
                    fleet: PathBuf::from("/f/a"),
                })),
            })),
            Reply::Response(Response::Stop(response::Stop::AllOff)),
            Reply::Response(Response::Create(response::Create {
                directory: PathBuf::from("/f/work"),
                projects: vec![PathBuf::from("/c/app")],
                created: true,
                adopted: true,
            })),
            Reply::Status(Status::Fleet(status::Fleet::AlreadyServed {
                project: PathBuf::from("/c/app"),
                fleet: PathBuf::from("/f/work"),
            })),
            Reply::Status(Status::File(status::File::Missing {
                path: PathBuf::from("/x"),
            })),
        ];
        for reply in replies {
            let mut buf = Vec::new();
            write_reply(&mut buf, &reply).ok();
            assert_eq!(read_reply(&mut Cursor::new(buf)).ok(), Some(reply));
        }
    }

    #[test]
    fn every_watching_reply_round_trips() {
        let mut replies = vec![
            Reply::Response(Response::Events(response::Events::Steps {
                fleet: Some(PathBuf::from("/c/app")),
                events: vec![Event {
                    id: 1043,
                    occur_time_millis: 1_761_586_867_000,
                    step: Step::SessionStateChanged {
                        session: "s1".into(),
                        fleet: Some("/c/app".into()),
                        seat: Some("scribe".into()),
                        state: SessionState::Working,
                    },
                }],
                next: 1044,
            })),
            Reply::Response(Response::Events(response::Events::Steps {
                fleet: None,
                events: vec![],
                next: 9,
            })),
            Reply::Response(Response::Wait(response::Wait::Reached {
                on: Waited::Session {
                    session: "s1".into(),
                },
                session: "s1".into(),
                state: SessionState::Idle,
            })),
            Reply::Response(Response::Wait(response::Wait::Delivered {
                message: "m1".into(),
                session: "s1".into(),
            })),
            Reply::Response(Response::Wait(response::Wait::Waiting {
                on: Waited::Message {
                    message: "m1".into(),
                },
                next: 9,
                session: None,
                state: None,
            })),
        ];
        let refusals = [
            status::Wait::NoSuchName {
                name: "critic".into(),
                fleet: Some(PathBuf::from("/c/app")),
            },
            status::Wait::NoStates {
                name: "scribe".into(),
            },
            status::Wait::Gone {
                session: "s1".into(),
            },
            status::Wait::Ended {
                session: "s1".into(),
            },
            status::Wait::SeatRemoved {
                fleet: PathBuf::from("/c/app"),
                seat: "scribe".into(),
            },
            status::Wait::FleetRemoved {
                fleet: PathBuf::from("/c/app"),
            },
            status::Wait::Dropped {
                message: "m1".into(),
                why: Dropped::SessionEnded,
            },
        ];
        replies.extend(refusals.map(|r| Reply::Status(r.into())));
        for reply in replies {
            let mut buf = Vec::new();
            write_reply(&mut buf, &reply).ok();
            assert_eq!(read_reply(&mut Cursor::new(buf)).ok(), Some(reply));
        }
    }

    #[test]
    fn an_events_fields_sit_beside_its_id_on_the_wire() {
        let event = Event {
            id: 3,
            occur_time_millis: 5,
            step: Step::EveryFleetOn,
        };
        assert_eq!(
            serde_json::to_string(&event).ok().as_deref(),
            Some(r#"{"id":3,"occur_time_millis":5,"step":"EVERY_FLEET_ON"}"#)
        );
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
        let missing: Status = status::File::Missing {
            path: PathBuf::from("/x"),
        }
        .into();
        write_reply(&mut buf, &Reply::Status(missing)).ok();
        assert_eq!(
            buf,
            b"{\"status\":{\"file\":{\"missing\":{\"path\":\"/x\"}}}}\n"
        );
    }

    #[test]
    fn one_refusal_of_each_area_round_trips() {
        let why: Status = status::Fleet::AlsoClaimed {
            project: "/c/api".into(),
            fleet: "/f/other".into(),
        }
        .into();
        let refusals: Vec<Status> = vec![
            status::Fleet::InsideFleet {
                directory: "/f/a/b".into(),
                fleet: "/f/a".into(),
            }
            .into(),
            status::Project::CannotTurnOn {
                project: "/c/api".into(),
                fleet: "/f/wing".into(),
                why: Box::new(why),
            }
            .into(),
            status::File::Unreadable {
                path: "/c/x".into(),
                error: "denied".into(),
            }
            .into(),
            status::Session::NoSuchSeatOrSession {
                name: "s0123456789".into(),
                fleet: None,
            }
            .into(),
            status::Adapter::ReleaseMismatch {
                url: "https://r/m.json".into(),
                what: status::Mismatch::BuildMissing,
            }
            .into(),
            Status::Malformed {
                error: "eof".into(),
            },
        ];
        for status in refusals {
            let mut buf = Vec::new();
            write_reply(&mut buf, &Reply::Status(status.clone())).ok();
            let back = read_reply(&mut buf.as_slice()).ok();
            assert_eq!(back, Some(Reply::Status(status)));
        }
    }

    #[test]
    fn spawned_sessions_go_on_the_wire_by_role() {
        let mut buf = Vec::new();
        write_reply(&mut buf, &Reply::Response(Response::Overview(overview()))).ok();
        assert!(
            String::from_utf8_lossy(&buf)
                .contains(r#""spawned":[{"role":"worker","session":{"session":"s4""#),
            "{}",
            String::from_utf8_lossy(&buf)
        );
    }

    #[test]
    fn every_session_reply_round_trips() {
        let replies = [
            Reply::Response(Response::Spawn(response::Spawn::Spawned {
                session: "s1".into(),
                fleet: PathBuf::from("/c/wing"),
                role: "worker".into(),
                directory: PathBuf::from("/c/app"),
            })),
            Reply::Response(Response::Overview(overview())),
            Reply::Response(Response::End(response::End::Ended {
                session: "s1".into(),
                seat: None,
            })),
            Reply::Response(Response::End(response::End::Ended {
                session: "s1".into(),
                seat: Some("scribe".into()),
            })),
            Reply::Status(Status::Session(status::Session::AdapterFailed {
                adapter: PathBuf::from("/a/claude"),
                verb: Verb::Launch,
                fault: AdapterFault::Exited {
                    code: Some(1),
                    said: "claude: not found".into(),
                },
            })),
            Reply::Status(Status::Session(status::Session::MultiplexerCouldNotRun {
                error: "No such file or directory (os error 2)".into(),
            })),
            Reply::Status(Status::Session(status::Session::MultiplexerFailed {
                command: "tmux new-session".into(),
                said: "no".into(),
            })),
            Reply::Status(Status::Session(status::Session::NoFleetHere {
                directory: "/c/x".into(),
            })),
            Reply::Status(Status::Session(status::Session::NoSuchRole {
                role: "worker".into(),
                fleet: "/c/wing".into(),
            })),
            Reply::Status(Status::Session(status::Session::NotAProject {
                path: "/c/app/src".into(),
                fleet: "/c/wing".into(),
            })),
            Reply::Status(Status::Session(status::Session::SeatNotRunning {
                seat: "scribe".into(),
                fleet: "/c/app".into(),
            })),
            Reply::Status(Status::Session(status::Session::NoSuchSeatOrSession {
                name: "s0".into(),
                fleet: None,
            })),
            Reply::Status(Status::Session(status::Session::NoSuchSeatOrSession {
                name: "scrib".into(),
                fleet: Some("/c/app".into()),
            })),
        ];
        for reply in replies {
            let mut buf = Vec::new();
            write_reply(&mut buf, &reply).ok();
            assert_eq!(read_reply(&mut Cursor::new(buf)).ok(), Some(reply));
        }
    }

    fn listed(session: &str, state: SessionState) -> response::Listed {
        response::Listed {
            session: session.into(),
            adapter: PathBuf::from("/a/claude"),
            directory: PathBuf::from("/c/app"),
            state,
            launch_time_millis: 5,
            waiting: 0,
        }
    }

    /// An overview with one of everything.
    fn overview() -> response::Overview {
        response::Overview {
            fleets: vec![response::FleetOverview {
                directory: PathBuf::from("/c/app"),
                on: true,
                seats: vec![
                    response::SeatOverview {
                        name: "scribe".into(),
                        role: "writer".into(),
                        session: Some(listed(
                            "s1",
                            SessionState::NeedsYou {
                                reason: Some(Reason::Permission),
                            },
                        )),
                        older_role: true,
                        down: None,
                        waiting: 0,
                    },
                    response::SeatOverview {
                        name: "reader".into(),
                        role: "writer".into(),
                        session: None,
                        older_role: false,
                        down: Some(response::Down {
                            why: Some(Status::Session(status::Session::NoSuchSeatOrSession {
                                name: "s0".into(),
                                fleet: None,
                            })),
                            next_try_millis: 9,
                        }),
                        waiting: 0,
                    },
                ],
                orphans: vec![response::Orphan {
                    seat: "old".into(),
                    session: listed("s2", SessionState::Idle),
                }],
                spawned: vec![response::Spawned {
                    role: "worker".into(),
                    session: listed("s4", SessionState::Working),
                }],
            }],
            loose: vec![listed("s3", SessionState::Gone)],
            faults: vec![response::Fault {
                adapter: PathBuf::from("/a/codex"),
                fault: AdapterFault::TimedOut {
                    after_millis: 20_000,
                },
            }],
        }
    }

    #[test]
    fn the_overview_wire_leaves_out_what_is_empty() {
        let mut buf = Vec::new();
        let ask = Request::Overview {
            directory: PathBuf::from("/c"),
        };
        write_request(&mut buf, &ask).ok();
        assert_eq!(buf, b"{\"overview\":{\"directory\":\"/c\"}}\n");
        let mut buf = Vec::new();
        let seat = response::SeatOverview {
            name: "s".into(),
            role: "w".into(),
            session: None,
            older_role: false,
            down: None,
            waiting: 0,
        };
        let o = response::Overview {
            fleets: vec![response::FleetOverview {
                directory: PathBuf::from("/c"),
                on: false,
                seats: vec![seat],
                orphans: vec![],
                spawned: vec![],
            }],
            ..response::Overview::default()
        };
        write_reply(&mut buf, &Reply::Response(Response::Overview(o))).ok();
        assert_eq!(
            String::from_utf8_lossy(&buf),
            "{\"response\":{\"overview\":{\"fleets\":[{\"directory\":\"/c\",\"on\":false,\
             \"seats\":[{\"name\":\"s\",\"role\":\"w\"}]}]}}}\n"
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
