//! The agent adapter protocol, authoritative here (core.md § Boundaries).
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

/// The verbs an agent adapter answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verb {
    /// Say what to run to start the agent.
    Launch,
    /// Say what state each session's agent is in.
    Read,
}

impl Verb {
    /// Every verb, in the order of the table above.
    pub const ALL: [Verb; 2] = [Verb::Launch, Verb::Read];

    /// The verb `word` names, as an adapter receives it.
    #[must_use]
    pub fn parse(word: &str) -> Option<Verb> {
        Verb::ALL.into_iter().find(|verb| verb.as_str() == word)
    }

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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn verbs_are_spelled_as_the_adapter_receives_them() {
        assert_eq!(Verb::Launch.as_str(), "launch");
        assert_eq!(Verb::Read.as_str(), "read");
    }

    #[test]
    fn all_holds_every_verb_and_parse_reads_each_back() {
        // A new verb stops this match compiling: add it to ALL too.
        let counted = |verb: Verb| match verb {
            Verb::Launch | Verb::Read => 1,
        };
        assert_eq!(Verb::ALL.into_iter().map(counted).sum::<usize>(), 2);
        for verb in Verb::ALL {
            assert_eq!(Verb::parse(verb.as_str()), Some(verb));
        }
        assert_eq!(Verb::parse("Launch"), None);
        assert_eq!(Verb::parse(""), None);
    }
}
