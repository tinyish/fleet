//! tinyfleet's state: one SQLite file per user, written only by the
//! supervisor.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use fleet_core::fleets::{Change, Fleet};
use fleet_core::tools::ToolError;
use rusqlite::{Connection, params};

mod step;

pub use step::Step;

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
    /// An outside tool failed part way; nothing was written.
    Tool(ToolError),
}

/// One session tinyfleet launched and has not stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRow {
    /// tinyfleet's id for it, also its tmux session's name.
    pub id: String,
    /// The adapter program that launched it, and reads it.
    pub adapter: PathBuf,
    /// Where its agent works.
    pub directory: PathBuf,
    /// When it was launched, by the supervisor's clock.
    pub launch_time_millis: i64,
    /// The pane its agent runs in, as tmux names it (`%12`).
    pub pane: String,
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
        let conn = Connection::open(state_dir.join(fleet_core::paths::STATE_FILE))?;
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
                 CREATE TABLE sessions (
                     id                 TEXT    PRIMARY KEY,
                     adapter            TEXT    NOT NULL,
                     directory          TEXT    NOT NULL,
                     launch_time_millis INTEGER NOT NULL,
                     pane               TEXT    NOT NULL
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

    /// Every session launched and not stopped, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn sessions(&self) -> Result<Vec<SessionRow>, StateError> {
        let mut query = self.conn.prepare(
            "SELECT id, adapter, directory, launch_time_millis, pane FROM sessions
             ORDER BY launch_time_millis, id",
        )?;
        let rows = query.query_map([], |row| {
            Ok(SessionRow {
                id: row.get(0)?,
                adapter: PathBuf::from(row.get::<_, String>(1)?),
                directory: PathBuf::from(row.get::<_, String>(2)?),
                launch_time_millis: row.get(3)?,
                pane: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl State {
    /// Runs `change` in one transaction: every write it makes, and every
    /// step it records, commits together, or none of them does. Every write
    /// to state goes through here, so a change and the step that records it
    /// never part.
    ///
    /// # Errors
    ///
    /// When SQLite fails, or `change` returns an error; then nothing
    /// changes.
    pub fn write<T>(
        &mut self,
        change: impl FnOnce(&mut Write<'_>) -> Result<T, StateError>,
    ) -> Result<T, StateError> {
        let mut write = Write {
            tx: self.conn.transaction()?,
        };
        let done = change(&mut write)?;
        write.tx.commit()?;
        Ok(done)
    }
}

/// One transaction's writes, from [`State::write`]. Dropped without
/// committing, it rolls back.
pub struct Write<'a> {
    tx: rusqlite::Transaction<'a>,
}

impl Write<'_> {
    /// Registers `fleet`, its projects on, and runs `write_config`: if writing
    /// the config fails, the transaction fails with it.
    ///
    /// # Errors
    ///
    /// When SQLite fails, a path is not UTF-8, or `write_config` fails.
    pub fn create_fleet(
        &mut self,
        fleet: &Fleet,
        write_config: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), StateError> {
        self.tx.execute(
            "INSERT INTO fleets (directory, create_time_millis) VALUES (?1, ?2)",
            params![utf8(&fleet.directory)?, now_millis()],
        )?;
        for project in &fleet.projects {
            self.tx.execute(
                "INSERT INTO projects (directory, fleet, active) VALUES (?1, ?2, 1)",
                params![utf8(project)?, utf8(&fleet.directory)?],
            )?;
        }
        write_config().map_err(StateError::Config)
    }

    /// Records `row`, then runs `launch`, which answers with the pane the
    /// agent runs in, and keeps it on the row: if `launch` fails, the
    /// transaction fails with it, so nothing is recorded.
    ///
    /// # Errors
    ///
    /// When SQLite fails, a path is not UTF-8, or `launch` fails.
    pub fn launch_session(
        &mut self,
        row: &SessionRow,
        launch: impl FnOnce() -> Result<String, ToolError>,
    ) -> Result<(), StateError> {
        self.tx.execute(
            "INSERT INTO sessions (id, adapter, directory, launch_time_millis, pane)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                row.id,
                utf8(&row.adapter)?,
                utf8(&row.directory)?,
                row.launch_time_millis,
                row.pane
            ],
        )?;
        let pane = launch().map_err(StateError::Tool)?;
        self.tx.execute(
            "UPDATE sessions SET pane = ?1 WHERE id = ?2",
            params![pane, row.id],
        )?;
        Ok(())
    }

    /// Forgets session `id`, then runs `stop`: if it fails, the session
    /// stays. False, running nothing, when there is no such session.
    ///
    /// # Errors
    ///
    /// When SQLite fails or `stop` fails.
    pub fn stop_session(
        &mut self,
        id: &str,
        stop: impl FnOnce() -> Result<(), ToolError>,
    ) -> Result<bool, StateError> {
        let forgot = self
            .tx
            .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        if forgot == 0 {
            return Ok(false);
        }
        stop().map_err(StateError::Tool)?;
        Ok(true)
    }

    /// Appends one step to the record.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the step cannot be stored.
    pub fn record(&mut self, step: &Step) -> Result<(), StateError> {
        let (name, detail) = step.to_row().map_err(StateError::Step)?;
        self.tx.execute(
            "INSERT INTO records (occur_time_millis, step, detail) VALUES (?1, ?2, ?3)",
            params![now_millis(), name, detail],
        )?;
        Ok(())
    }

    /// Removes the fleet in `directory`; its projects go with it.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn remove_fleet(&mut self, directory: &Path) -> Result<(), StateError> {
        self.tx.execute(
            "DELETE FROM fleets WHERE directory = ?1",
            params![utf8(directory)?],
        )?;
        Ok(())
    }

    /// Takes `projects` out of the fleet that serves them.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a path is not UTF-8.
    pub fn remove_projects(&mut self, projects: &[PathBuf]) -> Result<(), StateError> {
        for project in projects {
            self.tx.execute(
                "DELETE FROM projects WHERE directory = ?1",
                params![utf8(project)?],
            )?;
        }
        Ok(())
    }

    /// Turns one project on or off.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn set_active(&mut self, project: &Path, active: bool) -> Result<(), StateError> {
        self.tx.execute(
            "UPDATE projects SET active = ?1 WHERE directory = ?2",
            params![active, utf8(project)?],
        )?;
        Ok(())
    }

    /// Turns every project of the fleet in `directory` on or off.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn set_fleet_active(&mut self, directory: &Path, active: bool) -> Result<(), StateError> {
        self.tx.execute(
            "UPDATE projects SET active = ?1 WHERE fleet = ?2",
            params![active, utf8(directory)?],
        )?;
        Ok(())
    }

    /// Turns every project on or off.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn set_all_active(&mut self, active: bool) -> Result<(), StateError> {
        self.tx
            .execute("UPDATE projects SET active = ?1", params![active])?;
        Ok(())
    }

    /// Applies what re-reading every fleet's config changed. A project
    /// removed from one fleet and added to another moves, keeping its on or
    /// off; a project only added comes in off, and one only removed is
    /// dropped.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a path is not UTF-8.
    pub fn apply_reread(&mut self, changes: &[(PathBuf, Change)]) -> Result<(), StateError> {
        let added: Vec<&PathBuf> = changes.iter().flat_map(|(_, c)| &c.added).collect();
        for (_, change) in changes {
            for project in change.removed.iter().filter(|p| !added.contains(p)) {
                self.tx.execute(
                    "DELETE FROM projects WHERE directory = ?1",
                    params![utf8(project)?],
                )?;
            }
        }
        for (fleet, change) in changes {
            for project in &change.added {
                self.tx.execute(
                    "INSERT INTO projects (directory, fleet, active) VALUES (?1, ?2, 0)
                     ON CONFLICT (directory) DO UPDATE SET fleet = excluded.fleet",
                    params![utf8(project)?, utf8(fleet)?],
                )?;
            }
        }
        Ok(())
    }
}

fn utf8(path: &Path) -> Result<&str, StateError> {
    path.to_str()
        .ok_or_else(|| StateError::NotUtf8(path.to_path_buf()))
}

/// Now, in milliseconds since the Unix epoch: the clock every time in
/// state is stamped with.
#[must_use]
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_core::tools::ToolError;

    /// A state file in a fresh directory of its own.
    fn fresh(name: &str) -> (State, PathBuf) {
        let dir = std::env::temp_dir().join(format!("fleet-state-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let state = State::open(&dir).ok();
        assert!(state.is_some(), "opening {}", dir.display());
        (state.unwrap_or_else(|| unreachable!()), dir)
    }

    fn active(state: &State, project: &str) -> Option<bool> {
        state
            .conn
            .query_row(
                "SELECT active FROM projects WHERE directory = ?1",
                params![project],
                |row| row.get(0),
            )
            .ok()
    }

    fn records(state: &State) -> Option<i64> {
        state
            .conn
            .query_row("SELECT COUNT(*) FROM records", [], |row| row.get(0))
            .ok()
    }

    fn fleet(dir: &str, projects: &[&str]) -> Fleet {
        Fleet {
            directory: dir.into(),
            projects: projects.iter().map(PathBuf::from).collect(),
        }
    }

    #[test]
    fn a_write_that_fails_part_way_changes_nothing() {
        let (mut state, dir) = fresh("rollback");
        let f = fleet("/f/a", &["/c/app"]);
        assert!(state.write(|w| w.create_fleet(&f, || Ok(()))).is_ok());
        let before = records(&state);
        let failed = state.write(|w| {
            w.set_all_active(false)?;
            w.record(&Step::EveryFleetOff)?;
            Err::<(), _>(StateError::NotUtf8(PathBuf::from("/x")))
        });
        assert!(failed.is_err());
        assert_eq!(active(&state, "/c/app"), Some(true));
        assert_eq!(records(&state), before);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_project_moved_between_fleets_keeps_its_on_or_off() {
        let (mut state, dir) = fresh("moved");
        for f in [
            fleet("/f/a", &["/c/app", "/c/api"]),
            fleet("/f/b", &["/c/web"]),
        ] {
            assert!(state.write(|w| w.create_fleet(&f, || Ok(()))).is_ok());
        }
        let off = state.write(|w| w.set_active(Path::new("/c/api"), false));
        assert!(off.is_ok());
        let changes = [
            (
                PathBuf::from("/f/a"),
                Change {
                    added: vec![],
                    removed: vec!["/c/app".into(), "/c/api".into()],
                },
            ),
            (
                PathBuf::from("/f/b"),
                Change {
                    added: vec!["/c/app".into(), "/c/api".into(), "/c/new".into()],
                    removed: vec![],
                },
            ),
        ];
        assert!(state.write(|w| w.apply_reread(&changes)).is_ok());
        assert_eq!(active(&state, "/c/app"), Some(true));
        assert_eq!(active(&state, "/c/api"), Some(false));
        assert_eq!(active(&state, "/c/new"), Some(false));
        let fleets = state.fleets().unwrap_or_default();
        assert_eq!(
            fleets,
            vec![fleet("/f/b", &["/c/api", "/c/app", "/c/new", "/c/web"])]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removing_a_fleet_takes_its_projects_with_it() {
        let (mut state, dir) = fresh("remove");
        let f = fleet("/f/a", &["/c/api", "/c/app"]);
        assert!(state.write(|w| w.create_fleet(&f, || Ok(()))).is_ok());
        assert!(state.write(|w| w.remove_fleet(Path::new("/f/a"))).is_ok());
        assert_eq!(state.fleets().ok(), Some(vec![]));
        assert_eq!(active(&state, "/c/app"), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn taking_projects_keeps_the_fleet_and_the_rest() {
        let (mut state, dir) = fresh("take");
        let f = fleet("/f/a", &["/c/api", "/c/app"]);
        assert!(state.write(|w| w.create_fleet(&f, || Ok(()))).is_ok());
        assert!(
            state
                .write(|w| w.remove_projects(&[PathBuf::from("/c/app")]))
                .is_ok()
        );
        assert_eq!(state.fleets().ok(), Some(vec![fleet("/f/a", &["/c/api"])]));
        let _ = std::fs::remove_dir_all(dir);
    }

    fn row(id: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            adapter: "/a/claude".into(),
            directory: "/c/app".into(),
            launch_time_millis: 5,
            pane: String::new(),
        }
    }

    #[test]
    fn a_launched_session_keeps_its_pane() {
        let (mut state, _dir) = fresh("pane");
        let written = state.write(|w| w.launch_session(&row("s1"), || Ok("%7".into())));
        assert!(written.is_ok(), "{written:?}");
        let rows = state.sessions().unwrap_or_default();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pane, "%7");
    }

    fn launched(id: &str) -> Step {
        Step::SessionLaunched {
            session: id.into(),
            adapter: "/a/claude".into(),
            directory: "/c/app".into(),
            model: None,
            prompt: None,
        }
    }

    fn steps(state: &State) -> Vec<String> {
        let Ok(mut query) = state.conn.prepare("SELECT step FROM records ORDER BY id") else {
            return Vec::new();
        };
        query
            .query_map([], |row| row.get(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
    }

    fn tmux_said_no() -> ToolError {
        ToolError::Failed {
            command: "tmux new-session".into(),
            said: "no".into(),
        }
    }

    #[test]
    fn a_launched_session_is_listed_and_recorded() {
        let (mut state, dir) = fresh("session");
        let done = state.write(|w| {
            w.launch_session(&row("s1"), || Ok("%1".into()))?;
            w.record(&launched("s1"))
        });
        assert!(done.is_ok(), "{done:?}");
        assert_eq!(
            state.sessions().ok(),
            Some(vec![SessionRow {
                pane: "%1".into(),
                ..row("s1")
            }])
        );
        assert_eq!(steps(&state), ["SESSION_LAUNCHED"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_launch_that_fails_leaves_nothing() {
        let (mut state, dir) = fresh("session-fails");
        let failed = state.write(|w| {
            w.launch_session(&row("s1"), || Err(tmux_said_no()))?;
            w.record(&launched("s1"))
        });
        assert!(matches!(failed, Err(StateError::Tool(_))), "{failed:?}");
        assert_eq!(state.sessions().ok(), Some(vec![]));
        assert_eq!(steps(&state), Vec::<String>::new());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn stopping_forgets_the_session_and_stopping_again_says_there_was_none() {
        let (mut state, dir) = fresh("session-stop");
        assert!(
            state
                .write(|w| w.launch_session(&row("s1"), || Ok("%1".into())))
                .is_ok()
        );
        let ran = std::cell::Cell::new(0);
        let stop = || {
            ran.set(ran.get() + 1);
            Ok(())
        };
        assert_eq!(state.write(|w| w.stop_session("s1", stop)).ok(), Some(true));
        assert_eq!(state.sessions().ok(), Some(vec![]));
        assert_eq!(
            state.write(|w| w.stop_session("s1", stop)).ok(),
            Some(false)
        );
        assert_eq!(ran.get(), 1, "stop runs only for a session that was there");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_stop_that_fails_keeps_the_session() {
        let (mut state, dir) = fresh("session-stop-fails");
        assert!(
            state
                .write(|w| w.launch_session(&row("s1"), || Ok("%1".into())))
                .is_ok()
        );
        let failed = state.write(|w| w.stop_session("s1", || Err(tmux_said_no())));
        assert!(matches!(failed, Err(StateError::Tool(_))), "{failed:?}");
        assert_eq!(
            state.sessions().ok(),
            Some(vec![SessionRow {
                pane: "%1".into(),
                ..row("s1")
            }])
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
