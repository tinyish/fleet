//! tinyfleet's state: one SQLite file per user, written only by the
//! supervisor.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use fleet_core::fleets::Fleet;
use fleet_core::seats::{Role, Roster, Seat};
use fleet_core::tools::ToolError;
use rusqlite::{Connection, params};

mod fleets;
mod messages;
mod records;
mod sessions;

pub use messages::MessageRow;
pub use records::Bell;
pub use sessions::{Launched, Life, Named, SessionRow};

/// The schema version this code reads and writes. Until tinyfleet's first
/// release every schema change goes into version 1 (see core.md § Boundaries).
pub const SCHEMA_VERSION: i64 = 1;

/// The open state file.
pub struct State {
    conn: Connection,
    bell: Arc<Bell>,
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

/// A registered fleet's roles and seats, as its config last read, and
/// whether any of its projects is on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetRoster {
    /// The fleet's directory.
    pub fleet: PathBuf,
    /// Whether any of its projects is on, so its seats are kept filled.
    pub on: bool,
    /// Its roles and seats.
    pub roster: Roster,
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
                     detail            TEXT    NOT NULL,
                     fleet             TEXT
                 );
                 CREATE INDEX records_fleet ON records (fleet, id);
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
                     pane               TEXT    NOT NULL,
                     fleet              TEXT,
                     seat               TEXT,
                     role_digest        TEXT,
                     role               TEXT,
                     state              TEXT,
                     end_time_millis    INTEGER,
                     CHECK (seat IS NULL OR (fleet IS NOT NULL AND role IS NOT NULL)),
                     CHECK (role IS NULL OR fleet IS NOT NULL)
                 );
                 CREATE INDEX sessions_seat ON sessions (fleet, seat);
                 CREATE TABLE roles (
                     fleet     TEXT NOT NULL REFERENCES fleets (directory) ON DELETE CASCADE,
                     label     TEXT NOT NULL,
                     adapter   TEXT NOT NULL,
                     model     TEXT,
                     prompt    TEXT,
                     directory TEXT NOT NULL,
                     PRIMARY KEY (fleet, label)
                 );
                 CREATE TABLE seats (
                     fleet TEXT NOT NULL REFERENCES fleets (directory) ON DELETE CASCADE,
                     name  TEXT NOT NULL,
                     role  TEXT NOT NULL,
                     PRIMARY KEY (fleet, name)
                 );
                 CREATE TABLE messages (
                     id                TEXT    PRIMARY KEY,
                     fleet             TEXT,
                     seat              TEXT,
                     session           TEXT,
                     sender            TEXT    NOT NULL
                         CHECK (sender IN ('person', 'script', 'session')),
                     sender_session    TEXT,
                     text              TEXT    NOT NULL,
                     queue_time_millis INTEGER NOT NULL,
                     CHECK ((fleet IS NOT NULL AND seat IS NOT NULL AND session IS NULL)
                         OR (fleet IS NULL AND seat IS NULL AND session IS NOT NULL)),
                     CHECK ((sender = 'session') = (sender_session IS NOT NULL))
                 );
                 PRAGMA user_version = 1;
                 COMMIT;",
            )?;
        }
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        Ok(State {
            conn,
            bell: Arc::default(),
        })
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

    /// The roles and seats of the fleet in `fleet`, as its config last read;
    /// none for a fleet not registered.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn roster(&self, fleet: &Path) -> Result<Roster, StateError> {
        roster_in(&self.conn, fleet)
    }

    /// Every registered fleet's roster, by directory, and whether any of its
    /// projects is on.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn rosters(&self) -> Result<Vec<FleetRoster>, StateError> {
        let mut all = Vec::new();
        for fleet in self.fleets()? {
            let on = on_in(&self.conn, &fleet.directory)?;
            all.push(FleetRoster {
                roster: self.roster(&fleet.directory)?,
                fleet: fleet.directory,
                on,
            });
        }
        Ok(all)
    }
}

impl State {
    /// Runs `change` in one transaction: every write it makes, and every
    /// step it records, commits together, or none of them does. Every write
    /// to state goes through here, and each records its own steps, so a
    /// change and the step that records it never part.
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
        self.bell.ring();
        Ok(done)
    }
}

/// One transaction's writes, from [`State::write`]. Dropped without
/// committing, it rolls back.
pub struct Write<'a> {
    tx: rusqlite::Transaction<'a>,
}

fn utf8(path: &Path) -> Result<&str, StateError> {
    path.to_str()
        .ok_or_else(|| StateError::NotUtf8(path.to_path_buf()))
}

/// `path` as a step names it.
fn shown(path: &Path) -> String {
    path.display().to_string()
}

/// The roles and seats of the fleet in `fleet`, read through `conn`.
fn roster_in(conn: &Connection, fleet: &Path) -> Result<Roster, StateError> {
    let fleet = utf8(fleet)?;
    let mut roles = conn.prepare(
        "SELECT label, adapter, model, prompt, directory FROM roles
         WHERE fleet = ?1 ORDER BY label",
    )?;
    let roles = roles.query_map(params![fleet], |row| {
        Ok(Role {
            label: row.get(0)?,
            adapter: row.get(1)?,
            model: row.get(2)?,
            prompt: row.get(3)?,
            directory: PathBuf::from(row.get::<_, String>(4)?),
        })
    })?;
    let roles = roles.collect::<Result<_, _>>()?;
    let mut seats = conn.prepare("SELECT name, role FROM seats WHERE fleet = ?1 ORDER BY name")?;
    let seats = seats.query_map(params![fleet], |row| {
        Ok(Seat {
            name: row.get(0)?,
            role: row.get(1)?,
        })
    })?;
    Ok(Roster {
        roles,
        seats: seats.collect::<Result<_, _>>()?,
    })
}

/// Whether any project of the fleet in `fleet` is on, read through `conn`.
fn on_in(conn: &Connection, fleet: &Path) -> Result<bool, StateError> {
    Ok(conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM projects WHERE fleet = ?1 AND active = 1)",
        params![utf8(fleet)?],
        |row| row.get(0),
    )?)
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
    use fleet_core::agent::SessionState;
    use fleet_core::fleets::Change;
    use fleet_core::messages::{Dropped, Recipient, Sender};
    use fleet_core::protocol::response::ConfigFate;
    use fleet_core::record::Step;
    use fleet_core::seats::{Role, Seat, SeatKey};
    use fleet_core::status::{self, Status, Takes};
    use fleet_core::tools::ToolError;
    use std::sync::Arc;
    use std::time::Duration;

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

    /// Every step recorded, oldest first.
    fn recorded(state: &State) -> Vec<Step> {
        state
            .events(0, None, 1 << 20)
            .unwrap_or_default()
            .into_iter()
            .map(|e| e.step)
            .collect()
    }

    /// Every step recorded from `from` on, oldest first.
    fn recorded_from(state: &State, from: usize) -> Vec<Step> {
        recorded(state).split_off(from)
    }

    fn fleet(dir: &str, projects: &[&str]) -> Fleet {
        Fleet {
            directory: dir.into(),
            projects: projects.iter().map(PathBuf::from).collect(),
        }
    }

    /// Writes `f` with no roles or seats, its config written.
    fn create(state: &mut State, f: &Fleet) {
        let done = state.write(|w| w.create_fleet(f, &Roster::default(), false, || Ok(())));
        assert!(done.is_ok(), "{done:?}");
    }

    #[test]
    fn a_write_that_fails_part_way_changes_nothing() {
        let (mut state, dir) = fresh("rollback");
        create(&mut state, &fleet("/f/a", &["/c/app"]));
        let before = records(&state);
        let failed = state.write(|w| {
            w.all_off()?;
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
        create(&mut state, &fleet("/f/a", &["/c/app", "/c/api"]));
        create(&mut state, &fleet("/f/b", &["/c/web"]));
        let off = state.write(|w| w.set_active(Path::new("/c/api"), false));
        assert!(off.is_ok());
        let from = recorded(&state).len();
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
        let removed = |p: &str| Step::ProjectRemoved {
            fleet: "/f/a".into(),
            project: p.into(),
        };
        let added = |p: &str| Step::ProjectAdded {
            fleet: "/f/b".into(),
            project: p.into(),
        };
        assert_eq!(
            recorded_from(&state, from),
            [
                removed("/c/app"),
                removed("/c/api"),
                added("/c/app"),
                added("/c/api"),
                added("/c/new")
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removing_or_forgetting_a_fleet_takes_its_projects_and_records_it_once() {
        let (mut state, dir) = fresh("remove");
        create(&mut state, &fleet("/f/a", &["/c/app", "/c/api"]));
        create(&mut state, &fleet("/f/b", &["/c/web"]));
        let from = recorded(&state).len();
        let done = state.write(|w| {
            w.remove_fleet(Path::new("/f/a"), ConfigFate::Deleted)?;
            w.forget_fleet(Path::new("/f/b"))?;
            w.remove_fleet(Path::new("/f/a"), ConfigFate::Kept)?;
            w.forget_fleet(Path::new("/f/b"))
        });
        assert!(done.is_ok(), "{done:?}");
        assert_eq!(state.fleets().ok(), Some(vec![]));
        assert_eq!(active(&state, "/c/app"), None);
        assert_eq!(
            recorded_from(&state, from),
            [
                Step::FleetRemoved {
                    directory: "/f/a".into(),
                    projects: vec!["/c/api".into(), "/c/app".into()],
                    config: ConfigFate::Deleted,
                },
                Step::FleetForgotten {
                    directory: "/f/b".into()
                }
            ],
            "a fleet already gone records nothing"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn taking_projects_keeps_the_fleet_and_the_rest() {
        let (mut state, dir) = fresh("take");
        create(&mut state, &fleet("/f/a", &["/c/api", "/c/app"]));
        let from = recorded(&state).len();
        let taken = [PathBuf::from("/c/app")];
        assert!(
            state
                .write(|w| w.remove_projects(Path::new("/f/a"), &taken))
                .is_ok()
        );
        assert_eq!(state.fleets().ok(), Some(vec![fleet("/f/a", &["/c/api"])]));
        assert_eq!(
            recorded_from(&state, from),
            [Step::ProjectsTaken {
                fleet: "/f/a".into(),
                projects: vec!["/c/app".into()]
            }]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn on_and_off_record_each_project_with_its_fleet_and_every_fleet() {
        let (mut state, dir) = fresh("on-off");
        create(&mut state, &fleet("/f/a", &["/c/app"]));
        create(&mut state, &fleet("/f/b", &["/c/web"]));
        let from = recorded(&state).len();
        let done = state.write(|w| {
            w.supervisor_started("1.2.3", 42)?;
            w.set_active(Path::new("/c/app"), false)?;
            w.set_active(Path::new("/c/app"), true)?;
            w.set_active(Path::new("/c/none"), true)?;
            w.all_off()?;
            w.all_on(&[PathBuf::from("/f/a")], &[PathBuf::from("/f/b")])
        });
        assert!(done.is_ok(), "{done:?}");
        assert_eq!(active(&state, "/c/app"), Some(true));
        assert_eq!(active(&state, "/c/web"), Some(false));
        assert_eq!(
            recorded_from(&state, from),
            [
                Step::SupervisorStarted {
                    version: "1.2.3".into(),
                    pid: 42
                },
                Step::ProjectOff {
                    project: "/c/app".into(),
                    fleet: "/f/a".into()
                },
                Step::ProjectOn {
                    project: "/c/app".into(),
                    fleet: "/f/a".into()
                },
                Step::EveryFleetOff,
                Step::EveryFleetOn,
            ],
            "a project no fleet serves records nothing"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_broken_config_and_a_failed_launch_are_recorded() {
        let (mut state, dir) = fresh("broken");
        let why = Status::File(status::File::Missing {
            path: "/f/a/fleet.toml".into(),
        });
        let key = SeatKey {
            fleet: "/f/a".into(),
            seat: "scribe".into(),
        };
        let done = state.write(|w| {
            w.config_broken(Path::new("/f/a"), &why)?;
            w.seat_launch_failed(&key, &why)
        });
        assert!(done.is_ok(), "{done:?}");
        assert_eq!(
            recorded(&state),
            [
                Step::FleetConfigBroken {
                    fleet: "/f/a".into(),
                    why: why.clone()
                },
                Step::SeatLaunchFailed {
                    fleet: "/f/a".into(),
                    seat: "scribe".into(),
                    why
                }
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn row(id: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            adapter: "/a/claude".into(),
            directory: "/c/app".into(),
            launch_time_millis: 5,
            pane: String::new(),
            fleet: None,
            seat: None,
            role: None,
            role_digest: None,
            state: None,
            life: Life::Live,
        }
    }

    /// A seat's row: scribe of the fleet in `/c/app`.
    fn seat_row(id: &str) -> SessionRow {
        SessionRow {
            fleet: Some("/c/app".into()),
            seat: Some("scribe".into()),
            role: Some("w".into()),
            role_digest: Some("ab".into()),
            ..row(id)
        }
    }

    fn launched() -> Launched {
        Launched {
            adapter: "claude-code@2.1.285".into(),
            model: Some("opus".into()),
            prompt: None,
        }
    }

    /// Launches `row` in pane `%1`.
    fn launch(state: &mut State, row: &SessionRow) {
        let done = state.write(|w| w.launch_session(row, &launched(), || Ok("%1".into())));
        assert!(done.is_ok(), "{done:?}");
    }

    fn tmux_said_no() -> ToolError {
        ToolError::Failed {
            command: "tmux new-session".into(),
            said: "no".into(),
        }
    }

    #[test]
    fn a_launched_session_is_listed_with_its_pane_and_recorded() {
        let (mut state, dir) = fresh("session");
        launch(&mut state, &seat_row("s1"));
        assert_eq!(
            state.sessions().ok(),
            Some(vec![SessionRow {
                pane: "%1".into(),
                ..seat_row("s1")
            }])
        );
        assert_eq!(
            recorded(&state),
            [Step::SessionLaunched {
                session: "s1".into(),
                adapter: "claude-code@2.1.285".into(),
                program: "/a/claude".into(),
                directory: "/c/app".into(),
                model: Some("opus".into()),
                prompt: None,
                fleet: Some("/c/app".into()),
                seat: Some("scribe".into()),
                role: Some("w".into()),
            }]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_launch_that_fails_leaves_nothing() {
        let (mut state, dir) = fresh("session-fails");
        let failed =
            state.write(|w| w.launch_session(&row("s1"), &launched(), || Err(tmux_said_no())));
        assert!(matches!(failed, Err(StateError::Tool(_))), "{failed:?}");
        assert_eq!(state.sessions().ok(), Some(vec![]));
        assert_eq!(recorded(&state), []);
        let _ = std::fs::remove_dir_all(dir);
    }

    fn to_session(id: &str, message: &str) -> messages::MessageRow {
        messages::MessageRow {
            id: message.into(),
            to: Recipient::Session { session: id.into() },
            sender: Sender::Script,
            text: "hi".into(),
            queue_time_millis: 1,
        }
    }

    /// Queues message `message` for session `id`, recorded under `/c/app`.
    fn queue(state: &mut State, id: &str, message: &str) {
        let row = to_session(id, message);
        let done = state.write(|w| w.queue_message(&row, Some(Path::new("/c/app"))));
        assert!(done.is_ok(), "{done:?}");
    }

    fn dropped(message: &str, why: Dropped) -> Step {
        Step::MessageDropped {
            message: message.into(),
            why,
            fleet: Some("/c/app".into()),
        }
    }

    #[test]
    fn ending_records_it_drops_its_messages_and_ending_again_changes_nothing() {
        let (mut state, dir) = fresh("session-stop");
        launch(&mut state, &seat_row("s1"));
        queue(&mut state, "s1", "m1");
        let from = recorded(&state).len();
        let ran = std::cell::Cell::new(0);
        let stop = || {
            ran.set(ran.get() + 1);
            Ok(())
        };
        let first = state.write(|w| w.end_session("s1", stop)).ok();
        assert_eq!(first.flatten().map(|r| r.id), Some("s1".to_owned()));
        assert_eq!(state.sessions().ok(), Some(vec![]));
        assert_eq!(state.held().ok(), Some(vec![]));
        assert_eq!(state.write(|w| w.end_session("s1", stop)).ok(), Some(None));
        assert_eq!(ran.get(), 1, "end runs only for a session that was there");
        assert_eq!(
            recorded_from(&state, from),
            [
                Step::SessionEnded {
                    session: "s1".into(),
                    fleet: Some("/c/app".into()),
                    seat: Some("scribe".into())
                },
                dropped("m1", Dropped::SessionEnded)
            ],
            "and records only then"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_end_that_fails_keeps_the_session() {
        let (mut state, dir) = fresh("session-stop-fails");
        launch(&mut state, &row("s1"));
        let failed = state.write(|w| w.end_session("s1", || Err(tmux_said_no())));
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

    /// Ends session `id`, its tmux session stopped.
    fn end(state: &mut State, id: &str) {
        let done = state.write(|w| w.end_session(id, || Ok(())));
        assert!(done.is_ok(), "{done:?}");
    }

    /// Records session `id` read `now`.
    fn read(state: &mut State, id: &str, now: SessionState) {
        let done = state.write(|w| w.set_state(id, Some(now)));
        assert!(done.is_ok(), "{done:?}");
    }

    /// Session `id`'s life, as `session` and `named` read it back, and
    /// whether `sessions` lists it.
    fn life(state: &State, id: &str) -> (Option<Life>, bool) {
        let row = state.session(id).ok().flatten();
        let named = state.named(None, id, Takes::SeatOrSession).ok().flatten();
        assert_eq!(named, row.clone().map(Named::Session), "{id}");
        let rows = state.sessions().unwrap_or_default();
        (row.map(|r| r.life), rows.iter().any(|r| r.id == id))
    }

    #[test]
    fn a_sessions_life_is_kept_on_its_row() {
        let (mut state, dir) = fresh("life");
        let spawned = |id: &str| SessionRow {
            fleet: Some("/f/wing".into()),
            role: Some("worker".into()),
            ..row(id)
        };
        let seat = |id: &str, seat: &str, at: i64| SessionRow {
            seat: Some(seat.into()),
            launch_time_millis: at,
            ..seat_row(id)
        };
        let gone = SessionState::Gone;
        for id in ["p1", "p2", "p3", "p4", "p5"] {
            launch(&mut state, &spawned(id));
        }
        read(&mut state, "p2", gone);
        read(&mut state, "p3", gone);
        end(&mut state, "p3");
        end(&mut state, "p4");
        // Seat one is read gone; seat two read gone, then revived after
        // the clock stepped back; seat three ended, then revived; seat four
        // read gone three times; seat five read gone, then revived and that
        // one ended.
        launch(&mut state, &seat("k1", "one", 5));
        read(&mut state, "k1", gone);
        launch(&mut state, &seat("k2", "two", 5));
        read(&mut state, "k2", gone);
        launch(&mut state, &seat("k3", "two", 4));
        launch(&mut state, &seat("k4", "three", 5));
        end(&mut state, "k4");
        launch(&mut state, &seat("k5", "three", 6));
        for id in ["k6", "k7", "k8"] {
            launch(&mut state, &seat(id, "four", 5));
            read(&mut state, id, gone);
        }
        launch(&mut state, &seat("k9", "five", 5));
        read(&mut state, "k9", gone);
        launch(&mut state, &seat("k10", "five", 6));
        end(&mut state, "k10");
        end(&mut state, "p5");
        let from = recorded(&state).len();
        read(&mut state, "p5", SessionState::Idle);
        read(&mut state, "p5", gone);
        assert_eq!(recorded_from(&state, from), [], "a reading after its end");
        let cases = [
            ("p1", Life::Live, true),
            ("p2", Life::Gone, true),
            ("p3", Life::Ended, false),
            ("p4", Life::Ended, false),
            ("k1", Life::Gone, true),
            ("k2", Life::Gone, false),
            ("k3", Life::Live, true),
            ("k4", Life::Ended, false),
            ("k5", Life::Live, true),
            ("k6", Life::Gone, false),
            ("k7", Life::Gone, false),
            ("k8", Life::Gone, true),
            ("k9", Life::Gone, false),
            ("k10", Life::Ended, false),
            ("p5", Life::Ended, false),
        ];
        for (id, now, listed) in cases {
            assert_eq!(life(&state, id), (Some(now), listed), "{id}");
        }
        assert_eq!(life(&state, "zz"), (None, false), "never launched");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_state_is_kept_on_its_row_and_recorded_and_gone_drops_its_messages() {
        let (mut state, dir) = fresh("row-state");
        launch(&mut state, &seat_row("s1"));
        queue(&mut state, "s1", "m1");
        let from = recorded(&state).len();
        assert_eq!(state.sessions().ok().map(|r| r[0].state), Some(None));
        let idle = SessionState::Idle;
        assert!(state.write(|w| w.set_state("s1", Some(idle))).is_ok());
        assert_eq!(state.sessions().ok().map(|r| r[0].state), Some(Some(idle)));
        assert!(state.write(|w| w.set_state("s1", None)).is_ok());
        assert_eq!(state.sessions().ok().map(|r| r[0].state), Some(None));
        assert!(state.write(|w| w.set_state("s9", Some(idle))).is_ok());
        let gone = SessionState::Gone;
        assert!(state.write(|w| w.set_state("s1", Some(gone))).is_ok());
        assert_eq!(state.held().ok(), Some(vec![]));
        let changed = |state| Step::SessionStateChanged {
            session: "s1".into(),
            fleet: Some("/c/app".into()),
            seat: Some("scribe".into()),
            state,
        };
        assert_eq!(
            recorded_from(&state, from),
            [
                changed(idle),
                changed(gone),
                dropped("m1", Dropped::SessionGone)
            ],
            "clearing, or no such session, records nothing"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_message_is_recorded_queued_then_delivered_and_its_session_read_afresh() {
        let (mut state, dir) = fresh("deliver-steps");
        launch(&mut state, &seat_row("s1"));
        let idle = Some(SessionState::Idle);
        assert!(state.write(|w| w.set_state("s1", idle)).is_ok());
        let from = recorded(&state).len();
        queue(&mut state, "s1", "m1");
        let ids = ["m1".to_owned(), "m9".to_owned()];
        let delivered = state.write(|w| w.deliver(&ids, "s1", |_| Ok(())));
        assert_eq!(delivered.ok(), Some(vec!["m1".to_owned()]));
        assert_eq!(state.sessions().ok().map(|r| r[0].state), Some(None));
        let none = state.write(|w| w.deliver(&ids, "s1", |_| Ok(())));
        assert_eq!(none.ok(), Some(vec![]));
        let row = to_session("s1", "m1");
        assert_eq!(
            recorded_from(&state, from),
            [
                Step::MessageQueued {
                    message: "m1".into(),
                    to: row.to,
                    sender: row.sender,
                    text: row.text,
                    fleet: Some("/c/app".into())
                },
                Step::MessageDelivered {
                    message: "m1".into(),
                    session: "s1".into(),
                    fleet: Some("/c/app".into())
                }
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    fn role(label: &str, adapter: &str) -> Role {
        Role {
            label: label.into(),
            adapter: adapter.into(),
            model: None,
            prompt: None,
            directory: "/c/app".into(),
        }
    }

    fn seat(name: &str, role: &str) -> Seat {
        Seat {
            name: name.into(),
            role: role.into(),
        }
    }

    fn roster() -> Roster {
        Roster {
            roles: vec![
                Role {
                    model: Some("opus".into()),
                    prompt: Some("notes".into()),
                    ..role("w", "claude-code@2.1.285")
                },
                Role {
                    directory: "/c/app/x".into(),
                    ..role("x", "/a/x")
                },
            ],
            seats: vec![seat("critic", "x"), seat("scribe", "w")],
        }
    }

    #[test]
    fn a_fleets_roster_reads_back_and_follows_its_projects_on_or_off() {
        let (mut state, dir) = fresh("roster");
        let f = fleet("/c/app", &["/c/app"]);
        create(&mut state, &f);
        create(&mut state, &fleet("/c/blog", &["/c/blog"]));
        let done = state.write(|w| w.set_roster(&f.directory, &roster()));
        assert!(done.is_ok(), "{done:?}");
        assert_eq!(state.roster(&f.directory).ok(), Some(roster()));
        let on: Vec<(PathBuf, bool, usize)> = state
            .rosters()
            .unwrap_or_default()
            .into_iter()
            .map(|r| (r.fleet, r.on, r.roster.seats.len()))
            .collect();
        assert_eq!(
            on,
            [("/c/app".into(), true, 2), ("/c/blog".into(), true, 0)]
        );
        assert!(
            state
                .write(|w| w.set_active(Path::new("/c/app"), false))
                .is_ok()
        );
        let on: Vec<bool> = state
            .rosters()
            .unwrap_or_default()
            .iter()
            .map(|r| r.on)
            .collect();
        assert_eq!(on, [false, true]);
        assert!(
            state
                .write(|w| w.set_roster(&f.directory, &Roster::default()))
                .is_ok()
        );
        assert_eq!(state.roster(&f.directory).ok(), Some(Roster::default()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_fleet_created_with_a_roster_records_its_roles_then_seats_then_itself() {
        let (mut state, dir) = fresh("create-roster");
        let f = fleet("/c/app", &["/c/app"]);
        let done = state.write(|w| w.create_fleet(&f, &roster(), true, || Ok(())));
        assert!(done.is_ok(), "{done:?}");
        let fl = || "/c/app".to_owned();
        assert_eq!(
            recorded(&state),
            [
                Step::RoleAdded {
                    fleet: fl(),
                    role: "w".into()
                },
                Step::RoleAdded {
                    fleet: fl(),
                    role: "x".into()
                },
                Step::SeatAdded {
                    fleet: fl(),
                    seat: "critic".into(),
                    role: "x".into()
                },
                Step::SeatAdded {
                    fleet: fl(),
                    seat: "scribe".into(),
                    role: "w".into()
                },
                Step::FleetCreated {
                    directory: fl(),
                    projects: vec![fl()],
                    adopted: true
                }
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_roster_change_records_roles_around_their_seats_and_drops_a_removed_seats_messages() {
        let (mut state, dir) = fresh("roster-diff");
        let f = fleet("/c/app", &["/c/app"]);
        let had = Roster {
            roles: vec![role("w", "/a/w"), role("x", "/a/x")],
            seats: vec![
                seat("critic", "x"),
                seat("scribe", "w"),
                seat("tester", "w"),
            ],
        };
        let done = state.write(|w| w.create_fleet(&f, &had, false, || Ok(())));
        assert!(done.is_ok(), "{done:?}");
        let to_critic = messages::MessageRow {
            to: Recipient::Seat {
                fleet: "/c/app".into(),
                seat: "critic".into(),
            },
            ..to_session("s1", "m1")
        };
        let queued = state.write(|w| w.queue_message(&to_critic, Some(&f.directory)));
        assert!(queued.is_ok(), "{queued:?}");
        let from = recorded(&state).len();
        // Role x goes with its seat critic, w changes, k comes with tester
        // moved to it, and scribe stays as it was.
        let has = Roster {
            roles: vec![role("k", "/a/k"), role("w", "/a/w2")],
            seats: vec![seat("scribe", "w"), seat("tester", "k")],
        };
        assert!(state.write(|w| w.set_roster(&f.directory, &has)).is_ok());
        let fl = || "/c/app".to_owned();
        assert_eq!(
            recorded_from(&state, from),
            [
                Step::RoleAdded {
                    fleet: fl(),
                    role: "k".into()
                },
                Step::RoleChanged {
                    fleet: fl(),
                    role: "w".into()
                },
                Step::SeatChanged {
                    fleet: fl(),
                    seat: "tester".into(),
                    role: "k".into()
                },
                Step::SeatRemoved {
                    fleet: fl(),
                    seat: "critic".into()
                },
                Step::RoleRemoved {
                    fleet: fl(),
                    role: "x".into()
                },
                dropped("m1", Dropped::SeatRemoved)
            ]
        );
        let again = recorded(&state).len();
        assert!(state.write(|w| w.set_roster(&f.directory, &has)).is_ok());
        assert_eq!(recorded_from(&state, again), [], "nothing moved");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_seat_is_wanted_while_its_fleet_is_on_and_its_role_exists() {
        let (mut state, dir) = fresh("wanted");
        let f = fleet("/c/app", &["/c/app"]);
        let orphan = Roster {
            seats: vec![seat("ghost", "nobody")],
            ..roster()
        };
        let mut both = roster();
        both.seats.extend(orphan.seats);
        let done = state.write(|w| w.create_fleet(&f, &both, false, || Ok(())));
        assert!(done.is_ok(), "{done:?}");
        let key = |seat: &str| SeatKey {
            fleet: "/c/app".into(),
            seat: seat.into(),
        };
        let wanted = |state: &State| -> Vec<(String, String)> {
            state
                .wanted_seats()
                .unwrap_or_default()
                .into_iter()
                .map(|w| (w.key.seat, w.role.label))
                .collect()
        };
        assert_eq!(
            wanted(&state),
            [
                ("critic".to_owned(), "x".to_owned()),
                ("scribe".to_owned(), "w".to_owned())
            ],
            "a seat of no role is not wanted"
        );
        let digest = roster().roles[0].digest();
        let asked = |state: &mut State, seat: &str, digest: Option<&str>| {
            state.write(|w| w.seat_wanted(&key(seat), digest)).ok()
        };
        assert_eq!(asked(&mut state, "scribe", Some(&digest)), Some(true));
        assert_eq!(asked(&mut state, "scribe", Some("old")), Some(false));
        assert_eq!(asked(&mut state, "scribe", None), Some(true));
        assert_eq!(asked(&mut state, "ghost", None), Some(false));
        assert!(state.write(|w: &mut Write<'_>| w.all_off()).is_ok());
        assert_eq!(wanted(&state), []);
        assert_eq!(asked(&mut state, "scribe", None), Some(false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wingman_fleet_is_on_while_any_of_its_projects_is() {
        let (mut state, dir) = fresh("roster-any");
        create(&mut state, &fleet("/w", &["/c/api", "/c/web"]));
        assert!(
            state
                .write(|w| w.set_active(Path::new("/c/api"), false))
                .is_ok()
        );
        assert_eq!(state.rosters().ok().map(|r| r[0].on), Some(true));
        assert!(
            state
                .write(|w| w.set_active(Path::new("/c/web"), false))
                .is_ok()
        );
        assert_eq!(state.rosters().ok().map(|r| r[0].on), Some(false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn removing_a_fleet_takes_its_roster_and_its_seats_messages_but_not_their_sessions() {
        let (mut state, dir) = fresh("roster-rm");
        let f = fleet("/c/app", &["/c/app"]);
        let done = state.write(|w| w.create_fleet(&f, &roster(), false, || Ok(())));
        assert!(done.is_ok(), "{done:?}");
        launch(&mut state, &seat_row("s1"));
        let to_scribe = messages::MessageRow {
            to: Recipient::Seat {
                fleet: "/c/app".into(),
                seat: "scribe".into(),
            },
            ..to_session("s1", "m1")
        };
        let queued = state.write(|w| w.queue_message(&to_scribe, Some(&f.directory)));
        assert!(queued.is_ok(), "{queued:?}");
        let from = recorded(&state).len();
        assert!(state.write(|w| w.forget_fleet(&f.directory)).is_ok());
        assert_eq!(state.roster(&f.directory).ok(), Some(Roster::default()));
        assert_eq!(
            state.sessions().ok(),
            Some(vec![SessionRow {
                pane: "%1".into(),
                ..seat_row("s1")
            }])
        );
        assert_eq!(
            recorded_from(&state, from),
            [
                Step::FleetForgotten {
                    directory: "/c/app".into()
                },
                dropped("m1", Dropped::FleetRemoved)
            ]
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_spawned_session_keeps_its_fleet_and_role() {
        let (mut state, dir) = fresh("spawned-row");
        let spawned = SessionRow {
            fleet: Some("/f/wing".into()),
            role: Some("worker".into()),
            role_digest: Some("ab".into()),
            ..row("s1")
        };
        launch(&mut state, &spawned);
        assert_eq!(
            state.sessions().ok(),
            Some(vec![SessionRow {
                pane: "%1".into(),
                ..spawned
            }])
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_seat_needs_a_fleet_and_a_role_and_a_role_needs_a_fleet() {
        let (mut state, dir) = fresh("session-check");
        let bad = [
            SessionRow {
                fleet: Some("/f".into()),
                seat: Some("scribe".into()),
                ..row("s1")
            },
            SessionRow {
                seat: Some("scribe".into()),
                role: Some("w".into()),
                ..row("s2")
            },
            SessionRow {
                role: Some("w".into()),
                ..row("s3")
            },
        ];
        for r in &bad {
            let written = state.write(|w| w.launch_session(r, &launched(), || Ok("%1".into())));
            assert!(
                matches!(written, Err(StateError::Sqlite(_))),
                "{r:?}: {written:?}"
            );
        }
        assert_eq!(state.sessions().ok(), Some(vec![]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_fleet_is_registered_until_it_is_removed() {
        let (mut state, dir) = fresh("registered");
        let f = fleet("/f/a", &["/c/app"]);
        create(&mut state, &f);
        assert_eq!(
            state.write(|w| w.fleet_registered(&f.directory)).ok(),
            Some(true)
        );
        assert!(state.write(|w| w.forget_fleet(&f.directory)).is_ok());
        assert_eq!(
            state.write(|w| w.fleet_registered(&f.directory)).ok(),
            Some(false)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn events_are_read_from_an_id_for_one_fleet_with_the_supervisors_own() {
        let (mut state, dir) = fresh("events");
        let on = |p: &str, f: &str| Step::ProjectOn {
            project: p.into(),
            fleet: f.into(),
        };
        let first = state.next_id().unwrap_or_default();
        assert!(
            state
                .write(|w| {
                    w.record(&Step::EveryFleetOn)?;
                    w.record(&on("/c/a", "/c/a"))?;
                    w.record(&on("/c/b", "/c/b"))
                })
                .is_ok()
        );
        let ids = |fleet: Option<&str>, from: i64| {
            state
                .events(from, fleet.map(Path::new), 1 << 20)
                .ok()
                .map(|e| e.iter().map(|e| e.id - first).collect::<Vec<_>>())
        };
        assert_eq!(ids(None, first), Some(vec![0, 1, 2]));
        assert_eq!(ids(Some("/c/a"), first), Some(vec![0, 1]));
        assert_eq!(ids(Some("/c/b"), first + 1), Some(vec![2]));
        assert_eq!(state.next_id().ok(), Some(first + 3));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_batch_stops_at_its_budget_but_holds_at_least_one_step() {
        let (mut state, dir) = fresh("budget");
        let from = state.next_id().unwrap_or_default();
        let long = |m: &str| Step::MessageQueued {
            message: m.into(),
            to: Recipient::Session {
                session: "s1".into(),
            },
            sender: Sender::Script,
            text: "x".repeat(1000),
            fleet: None,
        };
        assert!(
            state
                .write(|w| {
                    w.record(&long("m1"))?;
                    w.record(&long("m2"))
                })
                .is_ok()
        );
        assert_eq!(state.events(from, None, 10).ok().map(|e| e.len()), Some(1));
        assert_eq!(
            state.events(from, None, 1 << 20).ok().map(|e| e.len()),
            Some(2)
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn steps_are_found_by_the_message_they_name() {
        let (mut state, dir) = fresh("steps-with");
        assert!(
            state
                .write(|w| {
                    w.record(&Step::MessageDelivered {
                        message: "m1".into(),
                        session: "s1".into(),
                        fleet: None,
                    })
                })
                .is_ok()
        );
        let count = |e: Result<Vec<_>, _>| e.ok().map(|e| e.len());
        assert_eq!(count(state.steps_with_message("m1")), Some(1));
        assert_eq!(count(state.steps_with_message("m9")), Some(0));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_bell_rings_on_a_commit_and_not_on_a_failed_write() {
        let (mut state, dir) = fresh("bell");
        let bell = state.bell();
        let before = bell.rung();
        assert!(state.write(|w| w.record(&Step::EveryFleetOn)).is_ok());
        assert_eq!(bell.rung(), before + 1);
        let failed: Result<(), StateError> = state.write(|w| {
            w.record(&Step::EveryFleetOff)?;
            Err(StateError::Newer(9))
        });
        assert!(failed.is_err());
        assert_eq!(bell.rung(), before + 1);
        let woke = std::thread::scope(|s| {
            let waiting = Arc::clone(&bell);
            let waiter = s.spawn(move || waiting.wait_past(before + 1, Duration::from_secs(5)));
            std::thread::sleep(Duration::from_millis(50));
            let _ = state.write(|w| w.record(&Step::EveryFleetOn));
            waiter.join().ok()
        });
        assert_eq!(woke, Some(before + 2));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_bell_not_rung_returns_at_its_time() {
        let bell = Bell::default();
        let started = std::time::Instant::now();
        assert_eq!(bell.wait_past(0, Duration::from_millis(50)), 0);
        assert!(started.elapsed() >= Duration::from_millis(50));
    }
}
