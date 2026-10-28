//! Agents' sessions as state keeps them: launched, listed and ended.

use std::path::{Path, PathBuf};

use fleet_core::agent::SessionState;
use fleet_core::tools::ToolError;
use rusqlite::{OptionalExtension, params};

use crate::{State, StateError, Write, utf8};

/// One session tinyfleet launched and has not ended.
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
}

/// The columns a row is read from, in `row_of`'s order.
const COLUMNS: &str =
    "id, adapter, directory, launch_time_millis, pane, fleet, seat, role, role_digest, state";

fn row_of(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionRow> {
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
        // A state this code no longer reads is not known.
        state: row
            .get::<_, Option<String>>(9)?
            .and_then(|s| serde_json::from_str(&s).ok()),
    })
}

impl State {
    /// Every session launched and not ended, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn sessions(&self) -> Result<Vec<SessionRow>, StateError> {
        let mut query = self.conn.prepare(&format!(
            "SELECT {COLUMNS} FROM sessions ORDER BY launch_time_millis, id"
        ))?;
        let rows = query.query_map([], row_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl Write<'_> {
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
        let pane = launch().map_err(StateError::Tool)?;
        self.tx.execute(
            "UPDATE sessions SET pane = ?1 WHERE id = ?2",
            params![pane, row.id],
        )?;
        Ok(())
    }

    /// Forgets session `id`, then runs `end`: if it fails, the session
    /// stays. The row it forgot; none, running nothing, when there is no
    /// such session.
    ///
    /// # Errors
    ///
    /// When SQLite fails or `end` fails.
    pub fn end_session(
        &mut self,
        id: &str,
        end: impl FnOnce() -> Result<(), ToolError>,
    ) -> Result<Option<SessionRow>, StateError> {
        let row = self
            .tx
            .query_row(
                &format!("SELECT {COLUMNS} FROM sessions WHERE id = ?1"),
                params![id],
                row_of,
            )
            .optional()?;
        if row.is_some() {
            self.tx
                .execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
            end().map_err(StateError::Tool)?;
        }
        Ok(row)
    }

    /// Keeps `state` as the last state recorded for session `session`; none
    /// clears it, so the next reading is recorded whatever it is.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the state cannot be stored.
    pub fn set_state(
        &mut self,
        session: &str,
        state: Option<SessionState>,
    ) -> Result<(), StateError> {
        let json = state
            .map(|s| serde_json::to_string(&s))
            .transpose()
            .map_err(StateError::Step)?;
        self.tx.execute(
            "UPDATE sessions SET state = ?1 WHERE id = ?2",
            params![json, session],
        )?;
        Ok(())
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
