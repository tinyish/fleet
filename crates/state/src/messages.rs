//! Messages held for a seat or a session until their agent is idle
//! (core.md § Primitives, Messages). Only held messages have a row: a
//! delivery or a drop deletes it, and the record keeps the history.

use std::path::{Path, PathBuf};

use fleet_core::messages::{self, Recipient, Sender};
use fleet_core::tools::ToolError;
use rusqlite::{ToSql, params};

use crate::{State, StateError, Write, utf8};

/// One held message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageRow {
    /// Its id: `m` and ten lowercase hex digits.
    pub id: String,
    /// Who it is for.
    pub to: Recipient,
    /// Who sent it.
    pub sender: Sender,
    /// What it says.
    pub text: String,
    /// When it was queued, by the supervisor's clock.
    pub queue_time_millis: i64,
}

/// Oldest first; messages queued in the same millisecond, in queue order.
const OLDEST_FIRST: &str = "ORDER BY queue_time_millis, rowid";

impl State {
    /// Every held message, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn held(&self) -> Result<Vec<MessageRow>, StateError> {
        let mut query = self.conn.prepare(&format!(
            "SELECT id, fleet, seat, session, sender, sender_session, text, queue_time_millis
             FROM messages {OLDEST_FIRST}"
        ))?;
        let rows = query.query_map([], |row| {
            Ok(MessageRow {
                id: row.get(0)?,
                to: recipient(row.get(1)?, row.get(2)?, row.get(3)?)?,
                sender: sender(&row.get::<_, String>(4)?, row.get(5)?)?,
                text: row.get(6)?,
                queue_time_millis: row.get(7)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

/// A row's recipient, from its `fleet`, `seat` and `session` columns; the
/// table's checks allow only a seat's two or a session's one.
fn recipient(
    fleet: Option<String>,
    seat: Option<String>,
    session: Option<String>,
) -> rusqlite::Result<Recipient> {
    match (fleet, seat, session) {
        (Some(fleet), Some(seat), None) => Ok(Recipient::Seat {
            fleet: PathBuf::from(fleet),
            seat,
        }),
        (None, None, Some(session)) => Ok(Recipient::Session { session }),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

/// A row's sender, from its `sender` and `sender_session` columns.
fn sender(kind: &str, session: Option<String>) -> rusqlite::Result<Sender> {
    match (kind, session) {
        ("person", None) => Ok(Sender::Person),
        ("script", None) => Ok(Sender::Script),
        ("session", Some(session)) => Ok(Sender::Session { session }),
        _ => Err(rusqlite::Error::InvalidQuery),
    }
}

impl Write<'_> {
    /// Holds `row`.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a path is not UTF-8.
    pub fn queue_message(&mut self, row: &MessageRow) -> Result<(), StateError> {
        let (fleet, seat, session) = match &row.to {
            Recipient::Seat { fleet, seat } => (Some(utf8(fleet)?), Some(seat.as_str()), None),
            Recipient::Session { session } => (None, None, Some(session.as_str())),
        };
        let (sender, sender_session) = match &row.sender {
            Sender::Person => ("person", None),
            Sender::Script => ("script", None),
            Sender::Session { session } => ("session", Some(session.as_str())),
        };
        self.tx.execute(
            "INSERT INTO messages
                 (id, fleet, seat, session, sender, sender_session, text, queue_time_millis)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                row.id,
                fleet,
                seat,
                session,
                sender,
                sender_session,
                row.text,
                row.queue_time_millis
            ],
        )?;
        Ok(())
    }

    /// Delivers those of `ids` still held: deletes them, then runs
    /// `type_in` with their texts as one batch, oldest first. Answers with
    /// their ids; with none still held, runs nothing. If `type_in` fails,
    /// the transaction fails with it, so they stay held.
    ///
    /// # Errors
    ///
    /// When SQLite fails, or `type_in` does.
    pub fn deliver(
        &mut self,
        ids: &[String],
        type_in: impl FnOnce(&str) -> Result<(), ToolError>,
    ) -> Result<Vec<String>, StateError> {
        let held: Vec<(String, String)> = {
            let mut query = self
                .tx
                .prepare(&format!("SELECT id, text FROM messages {OLDEST_FIRST}"))?;
            let rows = query.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        let still: Vec<(String, String)> = held
            .into_iter()
            .filter(|(id, _)| ids.contains(id))
            .collect();
        if still.is_empty() {
            return Ok(Vec::new());
        }
        for (id, _) in &still {
            self.tx
                .execute("DELETE FROM messages WHERE id = ?1", params![id])?;
        }
        type_in(&messages::batch(
            still.iter().map(|(_, text)| text.as_str()),
        ))
        .map_err(StateError::Tool)?;
        Ok(still.into_iter().map(|(id, _)| id).collect())
    }

    /// Drops the messages held for session `session`, answering with their
    /// ids, oldest first, for the caller to record.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn drop_for_session(&mut self, session: &str) -> Result<Vec<String>, StateError> {
        self.take("session = ?1", &[&session])
    }

    /// Drops the messages held for seat `seat` of the fleet in `fleet`.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn drop_for_seat(&mut self, fleet: &Path, seat: &str) -> Result<Vec<String>, StateError> {
        let fleet = utf8(fleet)?;
        self.take("fleet = ?1 AND seat = ?2", &[&fleet, &seat])
    }

    /// Drops the messages held for every seat of the fleet in `fleet`.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn drop_for_fleet(&mut self, fleet: &Path) -> Result<Vec<String>, StateError> {
        let fleet = utf8(fleet)?;
        self.take("fleet = ?1", &[&fleet])
    }

    /// Deletes the held messages `filter` picks, answering with their ids,
    /// oldest first.
    fn take(&mut self, filter: &str, args: &[&dyn ToSql]) -> Result<Vec<String>, StateError> {
        let ids: Vec<String> = {
            let mut query = self.tx.prepare(&format!(
                "SELECT id FROM messages WHERE {filter} {OLDEST_FIRST}"
            ))?;
            let rows = query.query_map(args, |row| row.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        self.tx
            .execute(&format!("DELETE FROM messages WHERE {filter}"), args)?;
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::State;
    use fleet_core::tools::ToolError;
    use std::cell::RefCell;

    fn fresh(name: &str) -> (State, PathBuf) {
        let dir = std::env::temp_dir().join(format!("fleet-msg-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let state = State::open(&dir).ok();
        assert!(state.is_some());
        (state.unwrap_or_else(|| unreachable!()), dir)
    }

    fn to_seat() -> Recipient {
        Recipient::Seat {
            fleet: "/c/app".into(),
            seat: "scribe".into(),
        }
    }

    fn to_session(session: &str) -> Recipient {
        Recipient::Session {
            session: session.into(),
        }
    }

    fn row(id: &str, to: Recipient, text: &str, at: i64) -> MessageRow {
        MessageRow {
            id: id.into(),
            to,
            sender: Sender::Session {
                session: "s9".into(),
            },
            text: text.into(),
            queue_time_millis: at,
        }
    }

    fn ids(state: &State) -> Vec<String> {
        state
            .held()
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.id)
            .collect()
    }

    fn failed() -> ToolError {
        ToolError::Failed {
            command: "tmux paste-buffer".into(),
            said: "no pane".into(),
        }
    }

    #[test]
    fn queued_messages_read_back_whole_oldest_first() {
        let (mut state, dir) = fresh("queue");
        let later = row("m2", to_seat(), "two", 5);
        let earlier = MessageRow {
            sender: Sender::Person,
            ..row("m1", to_session("s1"), "one", 5)
        };
        let script = MessageRow {
            sender: Sender::Script,
            ..row("m0", to_seat(), "zero", 9)
        };
        let queued = state.write(|w| {
            w.queue_message(&earlier)?;
            w.queue_message(&later)?;
            w.queue_message(&script)
        });
        assert!(queued.is_ok(), "{queued:?}");
        assert_eq!(
            state.held().ok(),
            Some(vec![earlier, later, script]),
            "same millis: queue order"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn deliver_types_the_ids_still_held_as_one_batch() {
        let (mut state, dir) = fresh("deliver");
        let queued = state.write(|w| {
            w.queue_message(&row("ma", to_seat(), "a\n", 1))?;
            w.queue_message(&row("mb", to_seat(), "b", 2))?;
            w.queue_message(&row("mc", to_seat(), "c", 3))
        });
        assert!(queued.is_ok());
        let typed = RefCell::new(String::new());
        let delivered = state.write(|w| {
            w.deliver(&["mc".into(), "ma".into(), "mz".into()], |text| {
                typed.replace(text.to_owned());
                Ok(())
            })
        });
        assert_eq!(delivered.ok(), Some(vec!["ma".to_owned(), "mc".to_owned()]));
        assert_eq!(typed.into_inner(), "a\n\nc");
        assert_eq!(ids(&state), ["mb"]);
        let none = state.write(|w| w.deliver(&["mz".into()], |_| Err(failed())));
        assert_eq!(none.ok(), Some(vec![]), "nothing held: nothing typed");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_failed_paste_leaves_every_message_held() {
        let (mut state, dir) = fresh("deliver-fails");
        assert!(
            state
                .write(|w| w.queue_message(&row("ma", to_seat(), "a", 1)))
                .is_ok()
        );
        let result = state.write(|w| w.deliver(&["ma".into()], |_| Err(failed())));
        assert!(matches!(result, Err(StateError::Tool(_))), "{result:?}");
        assert_eq!(ids(&state), ["ma"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn each_drop_takes_only_its_own_messages() {
        let (mut state, dir) = fresh("drops");
        let other_seat = Recipient::Seat {
            fleet: "/c/app".into(),
            seat: "critic".into(),
        };
        let other_fleet = Recipient::Seat {
            fleet: "/c/web".into(),
            seat: "scribe".into(),
        };
        let queued = state.write(|w| {
            w.queue_message(&row("m1", to_session("s1"), "x", 1))?;
            w.queue_message(&row("m2", to_session("s2"), "x", 2))?;
            w.queue_message(&row("m3", to_seat(), "x", 3))?;
            w.queue_message(&row("m4", other_seat, "x", 4))?;
            w.queue_message(&row("m5", other_fleet, "x", 5))
        });
        assert!(queued.is_ok());
        assert_eq!(
            state.write(|w| w.drop_for_session("s1")).ok(),
            Some(vec!["m1".to_owned()])
        );
        assert_eq!(
            state
                .write(|w| w.drop_for_seat(Path::new("/c/app"), "scribe"))
                .ok(),
            Some(vec!["m3".to_owned()])
        );
        assert_eq!(
            state.write(|w| w.drop_for_fleet(Path::new("/c/app"))).ok(),
            Some(vec!["m4".to_owned()])
        );
        assert_eq!(ids(&state), ["m2", "m5"]);
        let _ = std::fs::remove_dir_all(dir);
    }
}
