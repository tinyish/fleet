//! Agents' sessions as state keeps them: launched, read gone, ended, and
//! named. A session's row is never deleted; its life is read from it.

use std::path::{Path, PathBuf};

use fleet_core::agent::SessionState;
use fleet_core::messages::Dropped;
use fleet_core::record::Step;
use fleet_core::seats::SeatKey;
use fleet_core::status::{Status, Takes};
use fleet_core::tools::ToolError;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{State, StateError, Write, now_millis, shown, utf8};

/// Where a session is in its life, as state last recorded it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    /// Launched, and neither gone nor ended.
    Live,
    /// Its pane ended, as a tick read it.
    Gone,
    /// Someone or something said end.
    Ended,
}

/// One session tinyfleet launched, whatever its life.
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
    /// The fleet it was launched for: its seat's, or the one whose role it
    /// was spawned from. A fleet since removed leaves it loose.
    pub fleet: Option<PathBuf>,
    /// The seat it was launched for; none for a spawned session.
    pub seat: Option<String>,
    /// The label of the role it was launched from.
    pub role: Option<String>,
    /// The digest of that role, as it was then.
    pub role_digest: Option<String>,
    /// Its state as last recorded; none before its first reading.
    pub state: Option<SessionState>,
    /// Its life: ended once its end time is set, else gone once its state
    /// was recorded gone, else live. A launch is live.
    pub life: Life,
}

/// What a launch asked its adapter for, beyond the row: recorded with the
/// session launched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launched {
    /// The adapter, as it was named: a path, or `<name>@<version>`.
    pub adapter: String,
    /// The model it was asked to run.
    pub model: Option<String>,
    /// What the agent was first told.
    pub prompt: Option<String>,
}

/// What a name points at (core.md § Terms, name).
#[derive(Debug, PartialEq, Eq)]
pub enum Named {
    /// A seat of the fleet the name was given in.
    Seat(SeatKey),
    /// A session, whatever its life.
    Session(SessionRow),
    /// A message queued in the record.
    Message(String),
}

/// The columns a launch writes, in `row_of`'s order.
const COLUMNS: &str =
    "id, adapter, directory, launch_time_millis, pane, fleet, seat, role, role_digest, state";

/// The columns a row is read from, in `row_of`'s order.
const READ: &str = "id, adapter, directory, launch_time_millis, pane, fleet, seat, role, role_digest, \
     state, end_time_millis";

fn row_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
    // A state this code no longer reads is not known.
    let state: Option<SessionState> = row
        .get::<_, Option<String>>(9)?
        .and_then(|s| serde_json::from_str(&s).ok());
    let life = match (row.get::<_, Option<i64>>(10)?, state) {
        (Some(_), _) => Life::Ended,
        (None, Some(SessionState::Gone)) => Life::Gone,
        (None, _) => Life::Live,
    };
    Ok(SessionRow {
        id: row.get(0)?,
        adapter: PathBuf::from(row.get::<_, String>(1)?),
        directory: PathBuf::from(row.get::<_, String>(2)?),
        launch_time_millis: row.get(3)?,
        pane: row.get(4)?,
        fleet: row.get::<_, Option<String>>(5)?.map(PathBuf::from),
        seat: row.get(6)?,
        role: row.get(7)?,
        role_digest: row.get(8)?,
        state,
        life,
    })
}

impl State {
    /// Every session not ended, oldest launched first, except a seat's
    /// gone session once its seat has a newer one: the live ones, each
    /// spawned or orphaned session read gone until it is ended, and each
    /// seat's newest. Newer is launched later, by the order rows were
    /// written in, never by the clock.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn sessions(&self) -> Result<Vec<SessionRow>, StateError> {
        let gone = serde_json::to_string(&SessionState::Gone).map_err(StateError::Step)?;
        // Rows are never deleted, so a rowid is never reused; the seat's
        // index serves the newer row's lookup.
        let mut query = self.conn.prepare(&format!(
            "SELECT {READ} FROM sessions s
             WHERE end_time_millis IS NULL
               AND NOT (seat IS NOT NULL AND state = ?1 AND EXISTS (
                   SELECT 1 FROM sessions n
                   WHERE n.fleet = s.fleet AND n.seat = s.seat AND n.rowid > s.rowid))
             ORDER BY rowid"
        ))?;
        let rows = query.query_map(params![gone], row_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The row of session `id`, whatever its life; none for an id never
    /// launched.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn session(&self, id: &str) -> Result<Option<SessionRow>, StateError> {
        row_in(&self.conn, id)
    }

    /// What `name` points at, among the kinds `takes`: a seat of the fleet
    /// in `here`, else a session by its id, else, when the verb takes one,
    /// a message queued in the record. A kind the verb does not take is
    /// never looked up.
    ///
    /// # Errors
    ///
    /// When SQLite fails, or a path is not UTF-8.
    pub fn named(
        &self,
        here: Option<&Path>,
        name: &str,
        takes: Takes,
    ) -> Result<Option<Named>, StateError> {
        if let Some(fleet) = here
            && self.roster(fleet)?.seats.iter().any(|s| s.name == name)
        {
            return Ok(Some(Named::Seat(SeatKey {
                fleet: fleet.to_path_buf(),
                seat: name.to_owned(),
            })));
        }
        if let Some(row) = self.session(name)? {
            return Ok(Some(Named::Session(row)));
        }
        if takes == Takes::SeatOrSession {
            return Ok(None);
        }
        let queued = self
            .steps_with_message(name)?
            .iter()
            .any(|e| matches!(e.step, Step::MessageQueued { .. }));
        Ok(queued.then(|| Named::Message(name.to_owned())))
    }
}

impl Write<'_> {
    /// Records `row`, then runs `run`, which answers with the pane the
    /// agent runs in, keeps it on the row, and records the session
    /// launched, from the row and what `launched` asked: if `run` fails,
    /// the transaction fails with it, so nothing is recorded.
    ///
    /// # Errors
    ///
    /// When SQLite fails, a path is not UTF-8, or `run` fails.
    pub fn launch_session(
        &mut self,
        row: &SessionRow,
        launched: &Launched,
        run: impl FnOnce() -> Result<String, ToolError>,
    ) -> Result<(), StateError> {
        self.tx.execute(
            &format!(
                "INSERT INTO sessions ({COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
            ),
            params![
                row.id,
                utf8(&row.adapter)?,
                utf8(&row.directory)?,
                row.launch_time_millis,
                row.pane,
                row.fleet.as_deref().map(utf8).transpose()?,
                row.seat,
                row.role,
                row.role_digest,
                row.state
                    .map(|s| serde_json::to_string(&s))
                    .transpose()
                    .map_err(StateError::Step)?
            ],
        )?;
        let pane = run().map_err(StateError::Tool)?;
        self.tx.execute(
            "UPDATE sessions SET pane = ?1 WHERE id = ?2",
            params![pane, row.id],
        )?;
        self.record(&Step::SessionLaunched {
            session: row.id.clone(),
            adapter: launched.adapter.clone(),
            program: shown(&row.adapter),
            directory: shown(&row.directory),
            model: launched.model.clone(),
            prompt: launched.prompt.clone(),
            fleet: fleet_of(row),
            seat: row.seat.clone(),
            role: row.role.clone(),
        })
    }

    /// Ends session `id`, live or gone: sets its end time and runs
    /// `stop`, then records it ended and drops the messages held for it,
    /// recording each dropped as its session ended. If `stop` fails,
    /// nothing changes. The row as it was; none, running and recording
    /// nothing, for a session never launched or already ended.
    ///
    /// # Errors
    ///
    /// When SQLite fails or `stop` fails.
    pub fn end_session(
        &mut self,
        id: &str,
        stop: impl FnOnce() -> Result<(), ToolError>,
    ) -> Result<Option<SessionRow>, StateError> {
        let Some(row) = row_in(&self.tx, id)?.filter(|r| r.life != Life::Ended) else {
            return Ok(None);
        };
        self.tx.execute(
            "UPDATE sessions SET end_time_millis = ?1 WHERE id = ?2",
            params![now_millis(), id],
        )?;
        stop().map_err(StateError::Tool)?;
        self.record(&Step::SessionEnded {
            session: row.id.clone(),
            fleet: fleet_of(&row),
            seat: row.seat.clone(),
        })?;
        self.drop_for_session(
            id,
            row.fleet.as_deref().map(utf8).transpose()?,
            Dropped::SessionEnded,
        )?;
        Ok(Some(row))
    }

    /// Keeps `state` as the last state recorded for live session `id`,
    /// and, when it is some, records it changed; none clears it, recording
    /// nothing, so the next reading is recorded whatever it is. Gone is
    /// final: it makes the session gone, and drops the messages held for
    /// it, recording each dropped. Nothing, for a session not live.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the state cannot be stored.
    pub fn set_state(&mut self, id: &str, state: Option<SessionState>) -> Result<(), StateError> {
        let Some(row) = row_in(&self.tx, id)?.filter(|r| r.life == Life::Live) else {
            return Ok(());
        };
        let json = state
            .map(|s| serde_json::to_string(&s))
            .transpose()
            .map_err(StateError::Step)?;
        self.tx.execute(
            "UPDATE sessions SET state = ?1 WHERE id = ?2",
            params![json, id],
        )?;
        let Some(state) = state else {
            return Ok(());
        };
        self.record(&Step::SessionStateChanged {
            session: row.id.clone(),
            fleet: fleet_of(&row),
            seat: row.seat.clone(),
            state,
        })?;
        if state != SessionState::Gone {
            return Ok(());
        }
        self.drop_for_session(
            id,
            row.fleet.as_deref().map(utf8).transpose()?,
            Dropped::SessionGone,
        )
    }

    /// Records that launching `key`'s seat failed, for `why`.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the step cannot be stored.
    pub fn seat_launch_failed(&mut self, key: &SeatKey, why: &Status) -> Result<(), StateError> {
        self.record(&Step::SeatLaunchFailed {
            fleet: shown(&key.fleet),
            seat: key.seat.clone(),
            why: why.clone(),
        })
    }

    /// Whether a fleet is registered in `directory`, as this transaction
    /// sees it.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn fleet_registered(&self, directory: &Path) -> Result<bool, StateError> {
        Ok(self.tx.query_row(
            "SELECT EXISTS (SELECT 1 FROM fleets WHERE directory = ?1)",
            params![utf8(directory)?],
            |row| row.get(0),
        )?)
    }
}

/// Session `id`'s row, read through `conn`; none for no such session.
pub(crate) fn row_in(conn: &Connection, id: &str) -> Result<Option<SessionRow>, StateError> {
    Ok(conn
        .query_row(
            &format!("SELECT {READ} FROM sessions WHERE id = ?1"),
            params![id],
            row_of,
        )
        .optional()?)
}

/// `row`'s fleet, as a step names it.
pub(crate) fn fleet_of(row: &SessionRow) -> Option<String> {
    row.fleet.as_deref().map(shown)
}
