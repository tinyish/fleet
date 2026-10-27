//! Messages (core.md § Primitives, Messages): who sends one, who it is
//! for, why one is dropped, the rules its text keeps, how several go in as
//! one paste, and how long a session that took a paste is held.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::agent::SessionState;
use crate::status;

/// The variable every agent is launched with, holding its session's id, so
/// a message it sends names it as the sender.
pub const SESSION_VAR: &str = "FLEET_SESSION";

/// The longest text a message may hold, in bytes as it travels: JSON
/// escaping grows text, and the socket's line is 64 KiB.
pub const MAX_TEXT_BYTES: usize = 32 * 1024;

/// How long a session that took a paste, and still reads idle, takes no
/// more (core.md invariant 12).
pub const HOLD_MILLIS: i64 = 30_000;

/// Who sent a message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sender {
    /// A person at a terminal.
    Person,
    /// A program with no terminal.
    Script,
    /// Another agent's session.
    Session {
        /// Its id.
        session: String,
    },
}

/// Who a message is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recipient {
    /// A seat: whichever of its sessions is live when its agent is next
    /// idle.
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
}

/// Why a held message was dropped: it could no longer reach anyone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dropped {
    /// Its session was stopped, or cleared as gone.
    SessionEnded,
    /// Its seat was taken out of its fleet's config.
    SeatRemoved,
    /// Its seat's fleet was removed, or forgotten.
    FleetRemoved,
}

/// Refuses empty text, or text over [`MAX_TEXT_BYTES`] once escaped for
/// the socket, so a long text is refused, never a broken conversation.
///
/// # Errors
///
/// `EmptyText` or `TooLong`.
pub fn check_text(text: &str) -> Result<(), status::Message> {
    if text.trim().is_empty() {
        return Err(status::Message::EmptyText);
    }
    // Its JSON string, without the two quotes around it.
    let bytes = serde_json::to_string(text).map_or(text.len(), |s| s.len() - 2);
    if bytes > MAX_TEXT_BYTES {
        return Err(status::Message::TooLong {
            bytes,
            limit: MAX_TEXT_BYTES,
        });
    }
    Ok(())
}

/// The texts, oldest first, as one paste: each trimmed at its end, a blank
/// line between them.
#[must_use]
pub fn batch<'a>(texts: impl IntoIterator<Item = &'a str>) -> String {
    texts
        .into_iter()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// A session that took a paste: when, and whether a reading since has
/// shown it left idle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hold {
    /// When the paste went in.
    pub since_millis: i64,
    /// Whether the agent has read working or needing you since.
    pub left_idle: bool,
}

impl Hold {
    /// A hold from a paste at `since_millis`.
    #[must_use]
    pub fn new(since_millis: i64) -> Hold {
        Hold {
            since_millis,
            left_idle: false,
        }
    }

    /// Takes in a reading: working or needing you shows the paste was
    /// taken; starting, unknown or gone shows nothing.
    pub fn saw(&mut self, state: SessionState) {
        if matches!(state, SessionState::Working | SessionState::NeedsYou { .. }) {
            self.left_idle = true;
        }
    }

    /// Whether the session still takes no paste at `now_millis`.
    #[must_use]
    pub fn blocks(&self, now_millis: i64) -> bool {
        !self.left_idle && now_millis - self.since_millis < HOLD_MILLIS
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_or_blank_text_is_refused() {
        assert_eq!(check_text(""), Err(status::Message::EmptyText));
        assert_eq!(check_text(" \n\t"), Err(status::Message::EmptyText));
        assert_eq!(check_text("hi"), Ok(()));
    }

    #[test]
    fn text_up_to_the_limit_passes_and_one_byte_more_is_refused() {
        assert_eq!(check_text(&"a".repeat(MAX_TEXT_BYTES)), Ok(()));
        assert_eq!(
            check_text(&"a".repeat(MAX_TEXT_BYTES + 1)),
            Err(status::Message::TooLong {
                bytes: MAX_TEXT_BYTES + 1,
                limit: MAX_TEXT_BYTES
            })
        );
    }

    #[test]
    fn text_is_measured_as_it_travels_escaped() {
        // A quote travels as two bytes: half the limit in quotes is over it.
        let quotes = "\"".repeat(MAX_TEXT_BYTES / 2 + 1);
        assert_eq!(
            check_text(&quotes),
            Err(status::Message::TooLong {
                bytes: MAX_TEXT_BYTES + 2,
                limit: MAX_TEXT_BYTES
            })
        );
        // An escape character travels as six (`\u001b`).
        let ansi = "\u{1b}[0m".repeat(MAX_TEXT_BYTES / 9 + 1);
        assert!(check_text(&ansi).is_err());
        let fits = "\"".repeat(MAX_TEXT_BYTES / 2);
        assert_eq!(check_text(&fits), Ok(()));
    }

    #[test]
    fn a_batch_is_the_texts_in_order_a_blank_line_apart_each_trimmed_at_its_end() {
        assert_eq!(
            batch(["one\n", "two  ", "  three"]),
            "one\n\ntwo\n\n  three"
        );
        assert_eq!(batch(["alone\n"]), "alone");
    }

    #[test]
    fn a_hold_blocks_until_the_agent_leaves_idle_or_thirty_seconds_pass() {
        let mut hold = Hold::new(1_000);
        assert!(hold.blocks(1_000 + HOLD_MILLIS - 1));
        assert!(!hold.blocks(1_000 + HOLD_MILLIS));
        hold.saw(SessionState::Idle);
        hold.saw(SessionState::Starting);
        hold.saw(SessionState::Unknown);
        hold.saw(SessionState::Gone);
        assert!(
            hold.blocks(2_000),
            "none of these shows the paste was taken"
        );
        hold.saw(SessionState::Working);
        assert!(!hold.blocks(2_000));
        let mut asked = Hold::new(1_000);
        asked.saw(SessionState::NeedsYou { reason: None });
        assert!(!asked.blocks(2_000));
    }

    fn json(value: &impl serde::Serialize) -> String {
        serde_json::to_string(value).unwrap_or_default()
    }

    #[test]
    fn senders_and_recipients_are_snake_case_on_the_wire() {
        assert_eq!(json(&Sender::Person), r#""person""#);
        assert_eq!(
            json(&Sender::Session {
                session: "s1".into()
            }),
            r#"{"session":{"session":"s1"}}"#
        );
        assert_eq!(
            json(&Recipient::Seat {
                fleet: "/c/app".into(),
                seat: "scribe".into()
            }),
            r#"{"seat":{"fleet":"/c/app","seat":"scribe"}}"#
        );
        assert_eq!(json(&Dropped::SeatRemoved), r#""seat_removed""#);
    }
}
