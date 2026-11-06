//! Messages (core.md § Primitives, Messages): sending one queues it for a
//! seat of the fleet here or for a session by its id; delivering, which
//! the tending loop does each tick, types every message waiting for an idle
//! agent in as one paste, then holds that session until it has left idle or
//! 30 s pass. A message that can no longer reach anyone is dropped, and
//! recorded, in the change that ends its session, seat or fleet, or that
//! records its session gone.

use std::path::{Path, PathBuf};

use fleet_core::agent::SessionState;
use fleet_core::messages::{self, Recipient};
use fleet_core::protocol::{Reply, Response, Status, request, response};
use fleet_core::seats::SeatKey;
use fleet_core::status::{self, Takes};
use state::{Life, MessageRow, Named, SessionRow, State, StateError, now_millis};

use crate::names;
use crate::observe::Observed;
use crate::{Error, Supervisor, log, words};

/// Queues `ask`'s text for its recipient, with its step, or says why not.
/// The state lock is held from finding the recipient to queuing, so it
/// cannot go in between.
pub(crate) fn send(sup: &Supervisor, ask: request::Message) -> Result<Reply, Error> {
    let request::Message::Send {
        directory,
        to,
        text,
        sender,
    } = ask;
    if let Err(refusal) = messages::check_text(&text) {
        return Ok(Reply::Status(refusal.into()));
    }
    let mut state = sup.lock();
    let (to, fleet) = match recipient(&state, &directory, &to)? {
        Ok(found) => found,
        Err(status) => return Ok(Reply::Status(status)),
    };
    let row = MessageRow {
        id: crate::new_id('m'),
        to: to.clone(),
        sender,
        text,
        queue_time_millis: now_millis(),
    };
    state.write(|w| w.queue_message(&row, fleet.as_deref()))?;
    Ok(Reply::Response(Response::Message(
        response::Message::Queued {
            message: row.id,
            to,
        },
    )))
}

/// Who `to`, sent from `directory`, is, and their fleet: a seat of the
/// fleet here, else a live session.
fn recipient(
    state: &State,
    directory: &Path,
    to: &str,
) -> Result<Result<(Recipient, Option<PathBuf>), Status>, Error> {
    let resolved = names::resolve(state, directory, to, Takes::SeatOrSession)?;
    Ok(match resolved.named {
        Some(Named::Seat(SeatKey { fleet, seat })) => Ok((
            Recipient::Seat {
                fleet: fleet.clone(),
                seat,
            },
            Some(fleet),
        )),
        Some(Named::Session(row)) if row.life == Life::Live => {
            Ok((Recipient::Session { session: row.id }, row.fleet))
        }
        Some(Named::Session(row)) => Err(status::Message::SessionEnded { session: row.id }.into()),
        Some(Named::Message(_)) | None => Err(resolved.unknown().into()),
    })
}

/// Delivers what waits for each idle agent at `now_millis`, from the
/// tick's one reading of `rows`, which the holds have taken in: one paste
/// per session, then a hold (core.md invariant 12). Nothing is typed when
/// tmux or the adapter could not say the agent is idle.
pub(crate) fn deliver(sup: &Supervisor, rows: &[SessionRow], observed: &Observed, now_millis: i64) {
    let held = match sup.lock().held() {
        Ok(held) => held,
        Err(e) => return log(&words::tending_failed(&e.into())),
    };
    if held.is_empty() {
        return;
    }
    for row in rows {
        let ids = waiting_for(&held, row);
        let idle = observed.states.get(&row.id) == Some(&SessionState::Idle);
        // The holds are not locked while tmux types.
        let free = sup.holds().may_paste(&row.id, now_millis);
        if !ids.is_empty() && idle && free && type_in(sup, row, &ids) {
            sup.holds().pasted(&row.id, now_millis);
        }
    }
}

/// The ids of the held messages for `row`'s session, oldest first: those
/// sent to it, and those sent to its seat.
fn waiting_for(held: &[MessageRow], row: &SessionRow) -> Vec<String> {
    held.iter()
        .filter(|m| match &m.to {
            Recipient::Session { session } => *session == row.id,
            Recipient::Seat { fleet, seat } => names::of_seat(row, fleet, seat),
        })
        .map(|m| m.id.clone())
        .collect()
}

/// Types the messages `ids` still held into `row`'s pane as one paste, in
/// one transaction with their steps. Whether any went in.
fn type_in(sup: &Supervisor, row: &SessionRow, ids: &[String]) -> bool {
    let written = sup
        .lock()
        .write(|w| w.deliver(ids, &row.id, |text| sup.tools.mux.type_in(&row.pane, text)));
    match written {
        Ok(delivered) => !delivered.is_empty(),
        // tmux failed: the messages wait for the next tick.
        Err(StateError::Tool(_)) => false,
        Err(e) => {
            log(&words::tending_failed(&e.into()));
            false
        }
    }
}
