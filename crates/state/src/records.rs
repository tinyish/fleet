//! The record: every step, appended inside the write that made it, and read
//! back from an id as the events a caller follows (core.md § Primitives).

use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use fleet_core::protocol::Event;
use rusqlite::params;

use crate::{State, StateError, Step, Write, now_millis, utf8};

/// How many rows one read of the record takes at most, before the budget.
const ROWS: i64 = 500;

/// Rung after every commit, so a caller waiting for a new step waits
/// without the state lock.
#[derive(Debug, Default)]
pub struct Bell {
    rung: Mutex<u64>,
    cv: Condvar,
}

impl Bell {
    /// How many commits it has rung for.
    pub fn rung(&self) -> u64 {
        *self.rung.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Waits until it has rung past `seen`, or `within` passes; how many
    /// times it has rung then.
    pub fn wait_past(&self, seen: u64, within: Duration) -> u64 {
        let deadline = Instant::now() + within;
        let mut rung = self.rung.lock().unwrap_or_else(PoisonError::into_inner);
        while *rung <= seen {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                break;
            }
            rung = self
                .cv
                .wait_timeout(rung, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        *rung
    }

    /// Rings for one commit.
    pub(crate) fn ring(&self) {
        *self.rung.lock().unwrap_or_else(PoisonError::into_inner) += 1;
        self.cv.notify_all();
    }
}

impl State {
    /// The bell rung after each commit.
    #[must_use]
    pub fn bell(&self) -> Arc<Bell> {
        Arc::clone(&self.bell)
    }

    /// The id the next step will be recorded with.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn next_id(&self) -> Result<i64, StateError> {
        Ok(self
            .conn
            .query_row("SELECT COALESCE(MAX(id), 0) + 1 FROM records", [], |r| {
                r.get(0)
            })?)
    }

    /// The steps from id `from`, oldest first: with `fleet`, those about it
    /// and the supervisor's own; without, every one. It stops before
    /// `budget_bytes` of fields, but always holds the first step found.
    ///
    /// # Errors
    ///
    /// When SQLite fails, or a row is not a step this code knows.
    pub fn events(
        &self,
        from: i64,
        fleet: Option<&Path>,
        budget_bytes: usize,
    ) -> Result<Vec<Event>, StateError> {
        let fleet = fleet.map(utf8).transpose()?;
        let mut query = self.conn.prepare(
            "SELECT id, occur_time_millis, step, detail FROM records
             WHERE id >= ?1 AND (?2 IS NULL OR fleet = ?2 OR fleet IS NULL)
             ORDER BY id LIMIT ?3",
        )?;
        let rows = query.query_map(params![from, fleet, ROWS], row_parts)?;
        let mut events = Vec::new();
        let mut spent = 0;
        for row in rows {
            let (id, occur_time_millis, name, detail) = row?;
            spent += name.len() + detail.len() + 64;
            if spent > budget_bytes && !events.is_empty() {
                break;
            }
            events.push(event(id, occur_time_millis, &name, &detail)?);
        }
        Ok(events)
    }

    /// Every step whose `session` field is `session`, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails, or a row is not a step this code knows.
    pub fn steps_with_session(&self, session: &str) -> Result<Vec<Event>, StateError> {
        self.steps_with("$.session", session)
    }

    /// Every step whose `message` field is `message`, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails, or a row is not a step this code knows.
    pub fn steps_with_message(&self, message: &str) -> Result<Vec<Event>, StateError> {
        self.steps_with("$.message", message)
    }

    fn steps_with(&self, field: &str, value: &str) -> Result<Vec<Event>, StateError> {
        let mut query = self.conn.prepare(
            "SELECT id, occur_time_millis, step, detail FROM records
             WHERE json_extract(detail, ?1) = ?2 ORDER BY id",
        )?;
        let rows = query.query_map(params![field, value], row_parts)?;
        rows.map(|row| {
            let (id, occur_time_millis, name, detail) = row?;
            event(id, occur_time_millis, &name, &detail)
        })
        .collect()
    }

    /// Every step's name, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn steps(&self) -> Result<Vec<String>, StateError> {
        let mut query = self.conn.prepare("SELECT step FROM records ORDER BY id")?;
        let rows = query.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every step's detail, its fields as a JSON object, oldest first.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn step_details(&self) -> Result<Vec<String>, StateError> {
        let mut query = self
            .conn
            .prepare("SELECT detail FROM records ORDER BY id")?;
        let rows = query.query_map([], |row| row.get(0))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

impl Write<'_> {
    /// Appends one step to the record, with the fleet it is about.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the step cannot be stored.
    pub fn record(&mut self, step: &Step) -> Result<(), StateError> {
        let (name, detail) = step.to_row().map_err(StateError::Step)?;
        self.tx.execute(
            "INSERT INTO records (occur_time_millis, step, detail, fleet)
             VALUES (?1, ?2, ?3, ?4)",
            params![now_millis(), name, detail, step.fleet()],
        )?;
        Ok(())
    }
}

/// A record's row: its id, time, step name and detail.
type Parts = (i64, i64, String, String);

fn row_parts(row: &rusqlite::Row<'_>) -> rusqlite::Result<Parts> {
    Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
}

fn event(id: i64, occur_time_millis: i64, name: &str, detail: &str) -> Result<Event, StateError> {
    let step = Step::from_row(name, detail).map_err(StateError::Step)?;
    Ok(Event {
        id,
        occur_time_millis,
        step,
    })
}
