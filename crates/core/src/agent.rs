//! The agent adapter protocol, authoritative here (core.md § Boundaries),
//! and the state every session reads as, whatever its agent.
//!
//! An agent adapter is an external program. The supervisor runs it once per
//! request as `<adapter> <verb>`, writes one JSON object to its stdin and
//! reads one JSON object from its stdout. Exit 0 means the answer is on
//! stdout; any other exit is a failure, with what went wrong on stderr. A
//! run that does not answer within the host's time limit is killed.
//!
//! | Verb | Request | Answer |
//! | --- | --- | --- |
//! | `launch` | [`Launch`] | [`Launched`]: what to run; the adapter runs nothing |
//! | `read` | [`Read`] | [`Readings`]: each session's [`Activity`] |
//!
//! An adapter never touches tmux, and never says a session is gone: tmux
//! says that.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::tools::Pane;

/// The verbs an agent adapter answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
pub enum Verb {
    /// Say what to run to start the agent.
    Launch,
    /// Say what state each session's agent is in.
    Read,
}

impl Verb {
    /// The verb as the adapter receives it, its one argument.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Verb::Launch => "launch",
            Verb::Read => "read",
        }
    }
}

/// `launch`: start an agent in `directory`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launch {
    /// Where the agent works, absolute.
    pub directory: PathBuf,
    /// The model to run, in the agent's own naming; the adapter's default
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// The first thing the agent is told, if anything.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// The answer to `launch`: the program to run and what to add to its
/// environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Launched {
    /// The program and its arguments; the program by its absolute path.
    pub argv: Vec<String>,
    /// Variables the agent needs set.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
}

/// `read`: the sessions whose agents are running.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Read {
    /// One entry per session.
    pub sessions: Vec<SessionRef>,
}

/// One running session, as an adapter can find its agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRef {
    /// tinyfleet's id for the session.
    pub session: String,
    /// The agent's process id: the pane's.
    pub pid: u32,
    /// Where the agent works.
    pub directory: PathBuf,
}

/// The answer to `read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Readings {
    /// One per session the adapter could read; one it leaves out is unknown.
    pub sessions: Vec<Reading>,
}

/// What an adapter read of one session's agent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reading {
    /// tinyfleet's id for the session.
    pub session: String,
    /// What the agent is doing.
    pub activity: Activity,
    /// Why it needs a person, with [`Activity::NeedsYou`], when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<Reason>,
}

/// What an agent is doing, as its adapter reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    /// Launched, not ready yet.
    Starting,
    /// In a turn.
    Working,
    /// Waiting for a message.
    Idle,
    /// Blocked on what only a person can answer.
    NeedsYou,
    /// The adapter could not tell.
    Unknown,
}

/// Why an agent needs a person.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "specimen", derive(strum::EnumIter))]
pub enum Reason {
    /// A permission prompt.
    Permission,
    /// A question to the person.
    Question,
    /// The agent is logged out.
    LoggedOut,
    /// The agent hit its provider's usage limit.
    UsageLimit,
}

/// A running adapter failed, as facts; the CLI words them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(AdapterFaultKind), derive(strum::EnumIter))
)]
pub enum AdapterFault {
    /// The program could not be started.
    CouldNotRun {
        /// What the system said.
        error: String,
    },
    /// The program exited without answering.
    Exited {
        /// Its exit code; none when a signal ended it.
        code: Option<i32>,
        /// What it wrote on stderr.
        said: String,
    },
    /// The program did not answer in time, and was killed.
    TimedOut {
        /// How long it was given.
        after_millis: u64,
    },
    /// The program answered with something that is not the answer.
    BadAnswer {
        /// What the parser said.
        error: String,
    },
}

/// A session's state, the same for every agent (core.md § Primitives).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Launched, not ready yet.
    Starting,
    /// In a turn.
    Working,
    /// Waiting for a message.
    Idle,
    /// Blocked on what only a person can answer.
    NeedsYou {
        /// Why, when the adapter knows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<Reason>,
    },
    /// Its pane has ended.
    Gone,
    /// tinyfleet could not tell.
    Unknown,
}

/// How long a live session may read as starting before it is unknown.
pub const STARTING_GRACE_MILLIS: i64 = 30_000;

/// A session's state from its pane, as tmux lists it, and its adapter's
/// reading: a missing or dead pane is gone; a live one without a reading is
/// unknown, as is one still starting after [`STARTING_GRACE_MILLIS`].
#[must_use]
pub fn state_of(
    pane: Option<&Pane>,
    reading: Option<&Reading>,
    launch_time_millis: i64,
    now_millis: i64,
) -> SessionState {
    match (pane, reading) {
        (None | Some(Pane { dead: true, .. }), _) => SessionState::Gone,
        (Some(_), None) => SessionState::Unknown,
        (Some(_), Some(r)) => match r.activity {
            Activity::Starting if now_millis - launch_time_millis > STARTING_GRACE_MILLIS => {
                SessionState::Unknown
            }
            Activity::Starting => SessionState::Starting,
            Activity::Working => SessionState::Working,
            Activity::Idle => SessionState::Idle,
            Activity::NeedsYou => SessionState::NeedsYou { reason: r.reason },
            Activity::Unknown => SessionState::Unknown,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::Pane;

    fn pane(dead: bool) -> Pane {
        Pane {
            session: "s1".into(),
            pid: 7,
            dead,
        }
    }

    fn reading(activity: Activity) -> Reading {
        Reading {
            session: "s1".into(),
            activity,
            reason: None,
        }
    }

    #[test]
    fn a_missing_or_dead_pane_is_gone_whatever_the_adapter_says() {
        let r = reading(Activity::Working);
        assert_eq!(state_of(None, Some(&r), 0, 1), SessionState::Gone);
        assert_eq!(
            state_of(Some(&pane(true)), Some(&r), 0, 1),
            SessionState::Gone
        );
    }

    #[test]
    fn a_live_pane_without_a_reading_is_unknown() {
        assert_eq!(
            state_of(Some(&pane(false)), None, 0, 1),
            SessionState::Unknown
        );
    }

    #[test]
    fn starting_turns_unknown_after_the_grace() {
        let r = reading(Activity::Starting);
        let p = pane(false);
        assert_eq!(
            state_of(Some(&p), Some(&r), 0, STARTING_GRACE_MILLIS),
            SessionState::Starting
        );
        assert_eq!(
            state_of(Some(&p), Some(&r), 0, STARTING_GRACE_MILLIS + 1),
            SessionState::Unknown
        );
    }

    #[test]
    fn each_activity_of_a_live_pane_is_its_state() {
        let p = pane(false);
        for (activity, state) in [
            (Activity::Working, SessionState::Working),
            (Activity::Idle, SessionState::Idle),
            (Activity::Unknown, SessionState::Unknown),
        ] {
            assert_eq!(state_of(Some(&p), Some(&reading(activity)), 0, 1), state);
        }
    }

    #[test]
    fn needs_you_keeps_its_reason() {
        let r = Reading {
            session: "s1".into(),
            activity: Activity::NeedsYou,
            reason: Some(Reason::Permission),
        };
        assert_eq!(
            state_of(Some(&pane(false)), Some(&r), 0, 1),
            SessionState::NeedsYou {
                reason: Some(Reason::Permission)
            }
        );
    }

    #[test]
    fn the_wire_is_snake_case_and_leaves_out_what_is_absent() {
        let launch = Launch {
            directory: "/c/app".into(),
            model: None,
            prompt: None,
        };
        assert_eq!(
            serde_json::to_string(&launch).ok().as_deref(),
            Some(r#"{"directory":"/c/app"}"#)
        );
        let r = Reading {
            session: "s1".into(),
            activity: Activity::NeedsYou,
            reason: Some(Reason::LoggedOut),
        };
        assert_eq!(
            serde_json::to_string(&r).ok().as_deref(),
            Some(r#"{"session":"s1","activity":"needs_you","reason":"logged_out"}"#)
        );
        let launched: Option<Launched> = serde_json::from_str(r#"{"argv":["/bin/claude"]}"#).ok();
        assert_eq!(
            launched,
            Some(Launched {
                argv: vec!["/bin/claude".into()],
                env: BTreeMap::new(),
            })
        );
    }

    #[test]
    fn a_needs_you_state_without_a_reason_leaves_it_out() {
        let state = SessionState::NeedsYou { reason: None };
        assert_eq!(
            serde_json::to_string(&state).ok().as_deref(),
            Some(r#"{"needs_you":{}}"#)
        );
        assert_eq!(
            serde_json::to_string(&SessionState::Gone).ok().as_deref(),
            Some(r#""gone""#)
        );
    }

    #[test]
    fn verbs_are_spelled_as_the_adapter_receives_them() {
        assert_eq!(Verb::Launch.as_str(), "launch");
        assert_eq!(Verb::Read.as_str(), "read");
    }
}
