//! `fleet message`: queues text for a seat of the fleet here, or for a
//! session by its id; the supervisor types it in once the agent is idle. It
//! never starts the supervisor: starting it would turn every project on.

use std::io::{self, IsTerminal, Read};
use std::path::Path;

use fleet_core::messages::{self, Sender};
use fleet_core::protocol::{self, request};

use crate::client;
use crate::protocol::{Problem, Response, Status};

/// Queues `text`, or what is piped in, for `to`, from the current
/// directory.
pub(crate) fn run(state_dir: &Path, to: &str, text: Option<&str>) -> Result<Response, Status> {
    let text = text_of(text, io::stdin().is_terminal(), || {
        let mut piped = String::new();
        io::stdin().read_to_string(&mut piped).map(|_| piped)
    })?;
    messages::check_text(&text).map_err(|refusal| Status::Refused(refusal.into()))?;
    let (supervisor, directory) = client::from_here(state_dir)?;
    let terminal = io::stdin().is_terminal() && io::stderr().is_terminal();
    let sender = sender(std::env::var(messages::SESSION_VAR).ok(), terminal);
    let request = protocol::Request::Message(request::Message::Send {
        directory,
        to: to.to_owned(),
        text,
        sender,
    });
    let queued = supervisor.ask(&request, |answer| match answer {
        protocol::Response::Message(queued) => Some(queued),
        _ => None,
    })?;
    Ok(Response::MessageQueued(queued))
}

/// The text: the argument, else what `read` reads from stdin when stdin is
/// not a terminal.
fn text_of(
    arg: Option<&str>,
    stdin_terminal: bool,
    read: impl FnOnce() -> io::Result<String>,
) -> Result<String, Status> {
    match arg {
        Some(text) => Ok(text.to_owned()),
        None if stdin_terminal => Err(Status::Cannot(Problem::NoText)),
        None => read().map_err(|e| Status::Cannot(Problem::ReadText(e))),
    }
}

/// Who sends: the session `FLEET_SESSION` names, else a person at a
/// terminal, else a script.
fn sender(session: Option<String>, terminal: bool) -> Sender {
    match session.filter(|s| !s.is_empty()) {
        Some(session) => Sender::Session { session },
        None if terminal => Sender::Person,
        None => Sender::Script,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sender_is_the_session_named_else_a_person_at_a_terminal_else_a_script() {
        assert_eq!(
            sender(Some("s1".into()), true),
            Sender::Session {
                session: "s1".into()
            }
        );
        assert_eq!(sender(Some(String::new()), true), Sender::Person);
        assert_eq!(sender(None, true), Sender::Person);
        assert_eq!(sender(None, false), Sender::Script);
    }

    #[test]
    fn the_text_is_the_argument_else_what_is_piped_in() {
        let never = || -> io::Result<String> { Err(io::Error::other("read")) };
        assert_eq!(text_of(Some("hi"), true, never).ok().as_deref(), Some("hi"));
        assert_eq!(
            text_of(None, false, || Ok("piped\n".into()))
                .ok()
                .as_deref(),
            Some("piped\n")
        );
        assert!(matches!(
            text_of(None, true, never),
            Err(Status::Cannot(Problem::NoText))
        ));
        assert!(matches!(
            text_of(None, false, never),
            Err(Status::Cannot(Problem::ReadText(_)))
        ));
    }

    #[test]
    fn a_text_starting_with_a_dash_is_text() {
        use clap::Parser;
        let args = crate::Args::try_parse_from(["fleet", "message", "critic", "- fix the parser"]);
        assert!(
            matches!(
                args.as_ref().map(|a| &a.request),
                Ok(crate::protocol::Request::Message { text: Some(t), .. }) if t == "- fix the parser"
            ),
            "{args:?}"
        );
    }

    #[test]
    fn empty_text_is_refused_before_the_supervisor_is_asked() {
        let dir = std::env::temp_dir().join(format!("fleet-message-empty-{}", std::process::id()));
        assert!(matches!(
            run(&dir, "scribe", Some("  ")),
            Err(Status::Refused(protocol::Status::Message(
                fleet_core::status::Message::EmptyText
            )))
        ));
    }
}
