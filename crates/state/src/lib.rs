//! tinyfleet's state: one SQLite file per user, written only by the
//! supervisor.

use std::fmt;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, params};

mod step;

pub use step::{Record, Step};

/// The schema version this code reads and writes.
pub const SCHEMA_VERSION: i64 = 1;

/// The open state file.
pub struct State {
    conn: Connection,
}

/// The state file could not be opened, migrated or written.
#[derive(Debug)]
pub enum StateError {
    /// SQLite failed.
    Sqlite(rusqlite::Error),
    /// The file was written by a newer tinyfleet.
    Newer(i64),
    /// A row's step could not be written or read back.
    Step(serde_json::Error),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Sqlite(e) => write!(f, "state: {e}"),
            StateError::Newer(v) => write!(
                f,
                "state was written by a newer tinyfleet (schema {v}, this one knows {SCHEMA_VERSION})"
            ),
            StateError::Step(e) => write!(f, "state: a step in the record: {e}"),
        }
    }
}

impl From<rusqlite::Error> for StateError {
    fn from(e: rusqlite::Error) -> Self {
        StateError::Sqlite(e)
    }
}

impl State {
    /// Opens `state.sqlite3` in `state_dir`, creating and migrating it.
    ///
    /// # Errors
    ///
    /// When SQLite fails, or the file's schema is newer than this code's.
    pub fn open(state_dir: &Path) -> Result<State, StateError> {
        let conn = Connection::open(state_dir.join("state.sqlite3"))?;
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(StateError::Newer(version));
        }
        if version < 1 {
            conn.execute_batch(
                "BEGIN;
                 CREATE TABLE records (
                     id                INTEGER PRIMARY KEY,
                     occur_time_millis INTEGER NOT NULL,
                     step              TEXT    NOT NULL,
                     detail            TEXT    NOT NULL
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        Ok(State { conn })
    }

    /// Appends one step to the record.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn record(&mut self, step: &Step) -> Result<(), StateError> {
        let (name, detail) = step.to_row().map_err(StateError::Step)?;
        self.conn.execute(
            "INSERT INTO records (occur_time_millis, step, detail) VALUES (?1, ?2, ?3)",
            params![now_millis(), name, detail],
        )?;
        Ok(())
    }

    /// The whole record, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a row's step cannot be read back.
    pub fn records(&self) -> Result<Vec<Record>, StateError> {
        let mut query = self
            .conn
            .prepare("SELECT id, occur_time_millis, step, detail FROM records ORDER BY id")?;
        let rows = query.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        let mut records = Vec::new();
        for row in rows {
            let (id, occur_time_millis, name, detail) = row?;
            let step = Step::from_row(&name, &detail).map_err(StateError::Step)?;
            records.push(Record {
                id,
                occur_time_millis,
                step,
            });
        }
        Ok(records)
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
