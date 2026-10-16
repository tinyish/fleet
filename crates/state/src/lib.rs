//! tinyfleet's state: one SQLite file per user, written only by the
//! supervisor.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fleet_core::fleets::Fleet;
use rusqlite::{Connection, params};

mod step;

pub use step::{Record, Step};

/// The schema version this code reads and writes. Until tinyfleet's first
/// release every schema change goes into version 1 (see core.md § Boundaries).
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
    /// A path is not valid UTF-8, so it cannot be stored.
    NotUtf8(PathBuf),
    /// Writing the fleet's config file failed; nothing was registered.
    Config(io::Error),
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
                 CREATE TABLE fleets (
                     directory          TEXT    PRIMARY KEY,
                     create_time_millis INTEGER NOT NULL
                 );
                 CREATE TABLE projects (
                     directory TEXT    PRIMARY KEY,
                     fleet     TEXT    NOT NULL REFERENCES fleets (directory) ON DELETE CASCADE,
                     active    INTEGER NOT NULL
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(State { conn })
    }

    /// Every registered fleet, by directory, with its projects sorted.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn fleets(&self) -> Result<Vec<Fleet>, StateError> {
        let mut query = self.conn.prepare(
            "SELECT f.directory, p.directory FROM fleets f
             JOIN projects p ON p.fleet = f.directory
             ORDER BY f.directory, p.directory",
        )?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut fleets: Vec<Fleet> = Vec::new();
        for row in rows {
            let (directory, project) = row?;
            let directory = PathBuf::from(directory);
            match fleets.last_mut() {
                Some(last) if last.directory == directory => last.projects.push(project.into()),
                _ => fleets.push(Fleet {
                    directory,
                    projects: vec![project.into()],
                }),
            }
        }
        Ok(fleets)
    }

    /// Registers `fleet`, its projects on, and runs `write_config` inside the
    /// same transaction: if writing the config fails, nothing is registered.
    ///
    /// # Errors
    ///
    /// When SQLite fails, a path is not UTF-8, or `write_config` fails.
    pub fn create_fleet(
        &mut self,
        fleet: &Fleet,
        write_config: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), StateError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO fleets (directory, create_time_millis) VALUES (?1, ?2)",
            params![utf8(&fleet.directory)?, now_millis()],
        )?;
        for project in &fleet.projects {
            tx.execute(
                "INSERT INTO projects (directory, fleet, active) VALUES (?1, ?2, 1)",
                params![utf8(project)?, utf8(&fleet.directory)?],
            )?;
        }
        write_config().map_err(StateError::Config)?;
        tx.commit()?;
        Ok(())
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

fn utf8(path: &Path) -> Result<&str, StateError> {
    path.to_str()
        .ok_or_else(|| StateError::NotUtf8(path.to_path_buf()))
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}
