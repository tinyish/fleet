//! The words for what goes wrong with agents' sessions.

use fleet_core::agent::{AdapterFault, Verb};
use fleet_core::protocol;

use crate::ui::output::Ink;

/// Why the supervisor said no to a session request; none for a refusal
/// that is not about sessions.
pub(super) fn refused(status: &protocol::Status, ink: Ink) -> Option<String> {
    Some(match status {
        protocol::Status::AdapterFailed {
            adapter,
            verb,
            fault,
        } => format!(
            "the adapter {} failed to {}: {}",
            ink.path(adapter),
            match verb {
                Verb::Launch => "launch the agent",
                Verb::Read => "read its sessions",
            },
            fault_words(fault)
        ),
        protocol::Status::MultiplexerCouldNotRun { error } => {
            format!("{} could not be run: {error}", ink.accent("tmux"))
        }
        protocol::Status::MultiplexerFailed { command, said } if said.is_empty() => {
            format!("{} failed", ink.accent(command))
        }
        protocol::Status::MultiplexerFailed { command, said } => {
            format!("{} failed: {said}", ink.accent(command))
        }
        protocol::Status::NoSuchSession { session } => {
            format!("there is no session {}", ink.accent(session))
        }
        _ => return None,
    })
}

fn fault_words(fault: &AdapterFault) -> String {
    match fault {
        AdapterFault::CouldNotRun { error } => format!("it could not be run: {error}"),
        AdapterFault::Exited { code, said } => {
            let how = match code {
                Some(code) => format!("it exited with {code}"),
                None => "a signal ended it".to_owned(),
            };
            if said.is_empty() {
                how
            } else {
                format!("{how}: {said}")
            }
        }
        AdapterFault::TimedOut { after_millis } if after_millis % 1000 == 0 => {
            format!("it did not answer within {} seconds", after_millis / 1000)
        }
        AdapterFault::TimedOut { after_millis } => {
            format!("it did not answer within {after_millis} ms")
        }
        AdapterFault::BadAnswer { error } => format!("its answer could not be read: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::OUT;
    use fleet_core::agent::{AdapterFault, Verb};
    use std::path::PathBuf;

    fn adapter_failed(fault: AdapterFault) -> String {
        refused(
            &protocol::Status::AdapterFailed {
                adapter: PathBuf::from("/a/claude"),
                verb: Verb::Read,
                fault,
            },
            OUT.piped(),
        )
        .map(|w| console::strip_ansi_codes(&w).into_owned())
        .unwrap_or_default()
    }

    #[test]
    fn an_adapter_failure_says_which_adapter_what_it_was_asked_and_how_it_failed() {
        assert_eq!(
            adapter_failed(AdapterFault::Exited {
                code: Some(1),
                said: "claude: not found".into()
            }),
            "the adapter /a/claude failed to read its sessions: it exited with 1: claude: not found"
        );
        assert_eq!(
            adapter_failed(AdapterFault::TimedOut {
                after_millis: 20_000
            }),
            "the adapter /a/claude failed to read its sessions: it did not answer within 20 seconds"
        );
        assert_eq!(
            adapter_failed(AdapterFault::Exited {
                code: None,
                said: String::new()
            }),
            "the adapter /a/claude failed to read its sessions: a signal ended it"
        );
    }

    #[test]
    fn other_refusals_are_not_this_modules() {
        assert_eq!(
            refused(
                &protocol::Status::NotServed { path: "/x".into() },
                OUT.piped()
            ),
            None
        );
    }
}
