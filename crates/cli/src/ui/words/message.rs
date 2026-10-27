//! The words for sending a message: what was queued, and what goes wrong.

use fleet_core::messages::Recipient;
use fleet_core::protocol::response;
use fleet_core::status::Message;

use crate::ui::output::Ink;

/// What `fleet message` did.
pub(super) fn queued(queued: &response::Message, ink: Ink) -> String {
    let response::Message::Queued { message, to } = queued;
    let whom = match to {
        Recipient::Seat { seat, .. } => ink.accent(seat),
        Recipient::Session { session } => format!("the session {}", ink.accent(session)),
    };
    format!(
        "queued {} for {whom}; it is typed in once the agent is idle",
        ink.accent(message)
    )
}

/// Why the supervisor would not queue a message.
pub(super) fn refused(refusal: &Message, ink: Ink) -> String {
    match refusal {
        Message::NoSuchRecipient {
            to,
            fleet: Some(fleet),
        } => format!(
            "the fleet in {} has no seat {}, and no session has that id",
            ink.path(fleet),
            ink.accent(to)
        ),
        Message::NoSuchRecipient { to, fleet: None } => {
            format!("no session has the id {}", ink.accent(to))
        }
        Message::SessionGone { session } => {
            format!("the session {} has ended", ink.accent(session))
        }
        Message::EmptyText => "the message is empty".into(),
        Message::TooLong { bytes, limit } => {
            format!("the message is {bytes} bytes, more than the {limit} a message takes")
        }
    }
}

/// What to do about it.
pub(super) fn hint(refusal: &Message, ink: Ink) -> Option<String> {
    match refusal {
        Message::NoSuchRecipient { .. } => Some(format!(
            "run {} to see the seats and sessions there are",
            ink.accent("fleet status")
        )),
        Message::SessionGone { .. } | Message::EmptyText | Message::TooLong { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::ERR;
    use std::path::PathBuf;

    fn plain(text: &str) -> String {
        console::strip_ansi_codes(text).into_owned()
    }

    #[test]
    fn nobody_by_that_name_says_where_it_looked() {
        let here = Message::NoSuchRecipient {
            to: "scrib".into(),
            fleet: Some(PathBuf::from("/code/app")),
        };
        assert_eq!(
            plain(&refused(&here, ERR.piped())),
            "the fleet in /code/app has no seat scrib, and no session has that id"
        );
        let nowhere = Message::NoSuchRecipient {
            to: "s0".into(),
            fleet: None,
        };
        assert_eq!(
            plain(&refused(&nowhere, ERR.piped())),
            "no session has the id s0"
        );
        assert!(hint(&nowhere, ERR.piped()).is_some());
    }

    #[test]
    fn queued_says_for_whom() {
        use fleet_core::messages::Recipient;
        use fleet_core::protocol::response;
        let out = crate::ui::output::OUT.piped();
        let seat = response::Message::Queued {
            message: "m1".into(),
            to: Recipient::Seat {
                fleet: PathBuf::from("/code/app"),
                seat: "scribe".into(),
            },
        };
        assert_eq!(
            plain(&queued(&seat, out)),
            "queued m1 for scribe; it is typed in once the agent is idle"
        );
        let session = response::Message::Queued {
            message: "m2".into(),
            to: Recipient::Session {
                session: "s1".into(),
            },
        };
        assert_eq!(
            plain(&queued(&session, out)),
            "queued m2 for the session s1; it is typed in once the agent is idle"
        );
    }
}
