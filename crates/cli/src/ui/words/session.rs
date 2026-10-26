//! The words for what goes wrong with agents' sessions.

use fleet_core::agent::{AdapterFault, Verb};
use fleet_core::status::Session;

use crate::ui::output::Ink;

/// Why the supervisor said no to a session request.
pub(super) fn refused(refusal: &Session, ink: Ink) -> String {
    match refusal {
        Session::AdapterFailed {
            adapter,
            verb,
            fault,
        } => format!(
            "the adapter {} failed to {}: {}",
            ink.path(adapter),
            match verb {
                Verb::Launch => "launch the agent",
                Verb::Read => "read its sessions",
                Verb::Protocol => "say which protocol it speaks",
            },
            fault_words(fault)
        ),
        Session::MultiplexerCouldNotRun { error } => {
            format!("{} could not be run: {error}", ink.accent("tmux"))
        }
        Session::MultiplexerFailed { command, said } if said.is_empty() => {
            format!("{} failed", ink.accent(command))
        }
        Session::MultiplexerFailed { command, said } => {
            format!("{} failed: {said}", ink.accent(command))
        }
        Session::NoSuchSession { session } => {
            format!("there is no session {}", ink.accent(session))
        }
        Session::LaunchTooLong { bytes, limit } => format!(
            "launching the agent takes a {bytes}-byte {} command, more than the {limit} it \
             takes: shorten its role's prompt",
            ink.accent("tmux")
        ),
    }
}

/// What to do about it: nothing yet, for any session refusal.
pub(super) fn hint(refusal: &Session, _ink: Ink) -> Option<String> {
    match refusal {
        Session::AdapterFailed { .. }
        | Session::MultiplexerCouldNotRun { .. }
        | Session::MultiplexerFailed { .. }
        | Session::NoSuchSession { .. }
        | Session::LaunchTooLong { .. } => None,
    }
}

/// How an adapter failed, in words.
pub(super) fn fault_words(fault: &AdapterFault) -> String {
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
        let words = refused(
            &Session::AdapterFailed {
                adapter: PathBuf::from("/a/claude"),
                verb: Verb::Read,
                fault,
            },
            OUT.piped(),
        );
        console::strip_ansi_codes(&words).into_owned()
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
}
