//! The state every session reads as, whatever its agent, and how a running
//! adapter can fail. The agent adapter protocol itself is
//! `fleet-adapter-sdk`'s (core.md § Boundaries); it is re-exported here so
//! the rest of tinyfleet reaches it through core.

pub use fleet_adapter_sdk::Speaks;
pub use fleet_adapter_sdk::agent::{
    Activity, Launch, Launched, PROTOCOL, Read, Reading, Readings, Reason, SessionRef, Verb,
};

use serde::{Deserialize, Serialize};

use crate::tools::Pane;

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
}
