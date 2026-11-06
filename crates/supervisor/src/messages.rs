//! Messages (core.md § Primitives, Messages): sending one queues it for a
//! seat of the fleet here or for a session by its id; delivering, which
//! the tending loop does each tick, types every message waiting for an idle
//! agent in as one paste, then holds that session until it has left idle or
//! 30 s pass. A message that can no longer reach anyone is dropped, and
//! recorded, in the change that ends its session, seat or fleet.

use std::path::{Path, PathBuf};

use fleet_core::agent::SessionState;
use fleet_core::messages::{self, Dropped, Hold, Recipient};
use fleet_core::protocol::{Reply, Response, Status, request, response};
use fleet_core::seats::SeatKey;
use fleet_core::status::{self, Takes};
use state::{MessageRow, SessionRow, State, StateError, Step, Write, now_millis};

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
/// tick's one reading of `rows`: one paste per session, then a hold
/// (core.md invariant 12). Nothing is typed when tmux or the adapter could
/// not say the agent is idle.
pub(crate) fn deliver(sup: &Supervisor, rows: &[SessionRow], observed: &Observed, now_millis: i64) {
    let tending = &sup.tending;
    let held = match sup.lock().held() {
        Ok(held) => held,
        Err(e) => return log(&words::tending_failed(&e.into())),
    };
    let mut holds = tending.holds();
    holds.retain(|session, hold| hold.blocks(now_millis) && rows.iter().any(|r| r.id == *session));
    if held.is_empty() && holds.is_empty() {
        return tending.set_holds(holds);
    }
    let asked: Vec<&SessionRow> = rows
        .iter()
        .filter(|r| holds.contains_key(&r.id) || !waiting_for(&held, r).is_empty())
        .collect();
    for (session, hold) in &mut holds {
        if let Some(seen) = observed.states.get(session) {
            hold.saw(*seen);
        }
    }
    for row in &asked {
        let ids = waiting_for(&held, row);
        let idle = observed.states.get(&row.id) == Some(&SessionState::Idle);
        let blocked = holds.get(&row.id).is_some_and(|h| h.blocks(now_millis));
        if !ids.is_empty() && idle && !blocked && type_in(sup, row, &ids) {
            holds.insert(row.id.clone(), Hold::new(now_millis));
        }
    }
    tending.set_holds(holds);
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

/// Records each of `ids` dropped, for `why`, inside the caller's
/// transaction: the one whose change dropped them. `fleet` is the fleet of
/// who they were for.
pub(crate) fn dropped(
    w: &mut Write<'_>,
    ids: &[String],
    why: Dropped,
    fleet: Option<&Path>,
) -> Result<(), StateError> {
    ids.iter().try_for_each(|message| {
        w.record(&Step::MessageDropped {
            message: message.clone(),
            why,
            fleet: fleet.map(|f| f.display().to_string()),
        })
    })
}

/// Drops, and records, every message held for a seat of the fleet in
/// `fleet`, which is leaving: removed, or forgotten.
pub(crate) fn fleet_leaves(w: &mut Write<'_>, fleet: &Path) -> Result<(), StateError> {
    let ids = w.drop_for_fleet(fleet)?;
    dropped(w, &ids, Dropped::FleetRemoved, Some(fleet))
}
