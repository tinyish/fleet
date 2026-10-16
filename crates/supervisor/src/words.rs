//! Every line the supervisor writes to its log. A command's words are the
//! CLI's (`crates/cli/src/words.rs`): the supervisor answers commands with
//! facts and words only its own log, here (core.md invariant 9).

use std::io;

use fleet_core::paths::PathError;
use protocol::ProtocolError;
use state::StateError;

use crate::{Doing, Error};

/// What starts every line of the log.
pub(crate) const PREFIX: &str = "fleet supervise: ";

/// Said when `fleet supervise` runs where tinyfleet does not.
pub(crate) const UNSUPPORTED_OS: &str = "tinyfleet runs on macOS and Linux only";

/// Why there is no state directory to serve.
pub(crate) fn no_state_dir(error: &PathError) -> String {
    match error {
        PathError::NoHome => "HOME is not set, so there is nowhere to keep state".into(),
        PathError::NotAbsolute { var, value } => {
            format!("{var} must be an absolute path, not {}", value.display())
        }
    }
}

/// Why the supervisor stopped.
pub(crate) fn stopped(error: &Error) -> String {
    format!("stopped: {}", error_words(error))
}

/// Why one request went unanswered.
pub(crate) fn request_failed(error: &Error) -> String {
    format!("a request failed: {}", error_words(error))
}

/// Why a connection could not be accepted.
pub(crate) fn accept_failed(error: &io::Error) -> String {
    format!("accept failed: {error}")
}

fn error_words(error: &Error) -> String {
    match error {
        Error::AlreadyRunning => "already running for this state directory".into(),
        Error::Io(doing, e) => format!("{}: {e}", doing_words(*doing)),
        Error::Protocol(e) => protocol_words(e),
        Error::SocketPathTooLong(p) => format!("the socket path {} is too long", p.display()),
        Error::State(e) => state_words(e),
    }
}

fn doing_words(doing: Doing) -> &'static str {
    match doing {
        Doing::CreatingStateDir => "creating the state directory",
        Doing::SecuringStateDir => "securing the state directory",
        Doing::OpeningLock => "opening the lock",
        Doing::RemovingStaleSocket => "removing a stale socket",
        Doing::BindingSocket => "binding the socket",
        Doing::SettingTimeout => "setting the socket's timeout",
    }
}

fn protocol_words(error: &ProtocolError) -> String {
    match error {
        ProtocolError::Io(e) => e.to_string(),
        ProtocolError::Malformed(e) => format!("could not understand the request: {e}"),
        ProtocolError::Empty => "the other side closed without asking".into(),
        ProtocolError::TooLong => format!("a request longer than {} bytes", protocol::MAX_LINE),
    }
}

fn state_words(error: &StateError) -> String {
    match error {
        StateError::Sqlite(e) => format!("state: {e}"),
        StateError::Newer(v) => format!(
            "state was written by a newer tinyfleet (schema {v}, this one knows {})",
            state::SCHEMA_VERSION
        ),
        StateError::Step(e) => format!("state: a step in the record: {e}"),
        StateError::NotUtf8(p) => format!("{} is not valid UTF-8", p.display()),
        StateError::Config(e) => format!("writing fleet.toml: {e}"),
    }
}
