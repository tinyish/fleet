//! Messages (core.md § Primitives, Messages): sending one queues it for a
//! seat of the fleet here or for a session by its id; delivering, which
//! the tending loop does each tick, types every message waiting for an idle
//! agent in as one paste, then holds that session until it has left idle or
//! 30 s pass. A message that can no longer reach anyone is dropped, and
//! recorded, in the change that ends its session, seat or fleet.

use std::path::{Path, PathBuf};

use fleet_core::agent::SessionState;
use fleet_core::messages::{self, Recipient};
use fleet_core::protocol::{Reply, Response, Status, request, response};
use fleet_core::seats::SeatKey;
use fleet_core::status::{self, Takes};
use state::{MessageRow, SessionRow, State, StateError, Step, now_millis};

use crate::names::{self, Named};
use crate::observe::Observed;
use crate::{Error, Supervisor, Tools, log, words};

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
    let (to, fleet) = match recipient(&state, &sup.tools, &directory, &to)? {
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
    let step = Step::MessageQueued {
        message: row.id.clone(),
        to: to.clone(),
        sender: row.sender.clone(),
        text: row.text.clone(),
        fleet: fleet.map(|f| f.display().to_string()),
    };
    state.write(|w| {
        w.queue_message(&row)?;
        w.record(&step)
    })?;
    Ok(Reply::Response(Response::Message(
        response::Message::Queued {
            message: row.id,
            to,
        },
    )))
}

/// Who `to`, sent from `directory`, is, and their fleet: a seat of the
/// fleet here, else a session whose pane is alive.
fn recipient(
    state: &State,
    tools: &Tools,
    directory: &Path,
    to: &str,
) -> Result<Result<(Recipient, Option<PathBuf>), Status>, Error> {
    let resolved = names::resolve(state, directory, to, Takes::SeatOrSession)?;
    let row = match resolved.named {
        Some(Named::Seat(SeatKey { fleet, seat })) => {
            return Ok(Ok((
                Recipient::Seat {
                    fleet: fleet.clone(),
                    seat,
                },
                Some(fleet),
            )));
        }
        Some(Named::Session(row)) => row,
        Some(Named::Ended(session)) => {
            return Ok(Err(status::Message::SessionEnded { session }.into()));
        }
        Some(Named::Message(_)) | None => return Ok(Err(resolved.unknown().into())),
    };
    Ok(
        match crate::observe::alive(tools, std::slice::from_ref(&row)) {
            Ok(alive) if alive.get(&row.id).copied().unwrap_or(false) => {
                Ok((Recipient::Session { session: row.id }, row.fleet))
            }
            Ok(_) => Err(status::Message::SessionEnded { session: row.id }.into()),
            Err(e) => Err(crate::sessions::tool_status(e)),
        },
    )
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

/// Types the messages `ids` still held into `row`'s pane as one paste, with
/// a step for each, in one transaction. Whether any went in.
fn type_in(sup: &Supervisor, row: &SessionRow, ids: &[String]) -> bool {
    let written = sup.lock().write(|w| {
        let delivered = w.deliver(ids, |text| sup.tools.mux.type_in(&row.pane, text))?;
        // What it reads next is recorded afresh: after a paste, idle again
        // is the agent's answer.
        if !delivered.is_empty() {
            w.set_state(&row.id, None)?;
        }
        for message in &delivered {
            w.record(&Step::MessageDelivered {
                message: message.clone(),
                session: row.id.clone(),
                fleet: row.fleet.as_ref().map(|f| f.display().to_string()),
            })?;
        }
        Ok(delivered)
    });
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
