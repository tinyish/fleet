//! What wait and events say (core.md § How a command flows, wait /
//! events): polls of the record, from an id, answered at once or held
//! until a step commits.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// One recorded step, as a poll returns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Its id in the record: the cursor a caller asks again from.
    pub id: i64,
    /// When it was recorded, by the supervisor's clock.
    pub occur_time_millis: i64,
    /// The step, its fields beside `id` and `occur_time_millis` on the wire.
    #[serde(flatten)]
    pub step: crate::record::Step,
}

/// The longest a poll is held while there is nothing to answer yet, in
/// milliseconds.
pub const MAX_HOLD_MILLIS: u64 = 25_000;

/// How many bytes of steps one answer carries at most, under the socket's
/// 64 KiB line; one step always goes, however long.
pub const BATCH_BYTES: usize = 48 * 1024;

/// What a wait waits on, as its first poll resolved the name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Waited {
    /// A seat: whichever of its sessions is live.
    Seat {
        /// The seat's fleet, by its directory.
        fleet: PathBuf,
        /// The seat's name.
        seat: String,
    },
    /// One session.
    Session {
        /// Its id.
        session: String,
    },
    /// One message, until it is delivered.
    Message {
        /// Its id.
        message: String,
    },
}

/// The requests of wait and events, re-exported by `protocol::request`.
pub mod request {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    use crate::agent::StateName;

    /// What `fleet events` asks.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Events {
        /// The steps from `from` (the next one when absent) about the fleet
        /// holding `directory`, with the supervisor's own, or every step
        /// outside every fleet; held up to `hold_millis` (at most
        /// [`MAX_HOLD_MILLIS`](super::MAX_HOLD_MILLIS)) while there are
        /// none yet.
        Follow {
            /// The CLI's current directory, absolute.
            directory: PathBuf,
            /// The first record id wanted.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            from: Option<i64>,
            /// How long the poll may be held.
            hold_millis: u64,
        },
    }

    /// What `fleet wait` asks, once per poll.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Wait {
        /// The first poll: `name`, from `directory`, is a seat of the fleet
        /// holding it, else a session's id, else a message's.
        For {
            /// The CLI's current directory, absolute.
            directory: PathBuf,
            /// A seat's name, a session's id or a message's id.
            name: String,
            /// The states wanted; none for a message.
            states: Vec<StateName>,
            /// How long the poll may be held.
            hold_millis: u64,
        },
        /// A later poll: what the first resolved, from record id `from`.
        On {
            /// What is waited on.
            on: super::Waited,
            /// The states wanted; none for a message.
            states: Vec<StateName>,
            /// The first record id to judge.
            from: i64,
            /// How long the poll may be held.
            hold_millis: u64,
        },
    }
}

/// The replies of wait and events, re-exported by `protocol::response`.
pub mod response {
    use serde::{Deserialize, Serialize};
    use std::path::PathBuf;

    use crate::agent::SessionState;

    /// What `fleet events` got from one poll.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Events {
        /// The steps found, oldest first, possibly none: ask again from
        /// `next`.
        Steps {
            /// The fleet followed: the one holding the directory, if any.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            fleet: Option<PathBuf>,
            /// The steps.
            events: Vec<super::Event>,
            /// The record id to ask from next.
            next: i64,
        },
    }

    /// What one poll of `fleet wait` came to.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(rename_all = "snake_case")]
    pub enum Wait {
        /// A session of what was waited on reads one of the states.
        Reached {
            /// What was waited on.
            on: super::Waited,
            /// The session.
            session: String,
            /// Its state.
            state: SessionState,
        },
        /// The message was typed into `session`.
        Delivered {
            /// The message's id.
            message: String,
            /// The session it was typed into.
            session: String,
        },
        /// Not yet: ask again from `next`.
        Waiting {
            /// What is waited on.
            on: super::Waited,
            /// The record id to ask from next.
            next: i64,
            /// Its live session, when it has one.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            session: Option<String>,
            /// That session's last recorded state, when it has one.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            state: Option<SessionState>,
        },
    }
}
