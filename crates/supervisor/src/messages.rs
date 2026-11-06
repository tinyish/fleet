//! Messages (core.md § Primitives, Messages): sending one queues it for a
//! seat of the fleet here or for a session by its id; delivering, which
//! the tending loop does each tick, types every message waiting for an idle
//! agent in as one paste, then holds that session until it has left idle or
//! 30 s pass. A message that can no longer reach anyone is dropped, and
//! recorded, in the change that ends its session, seat or fleet.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::agent::SessionState;
use fleet_core::messages::{self, Dropped, Hold, Recipient, Sender};
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::seats::SeatKey;
use fleet_core::status::{self, Takes};
use state::{MessageRow, SessionRow, State, StateError, Step, Write, now_millis};

use crate::names::{self, Named};
use crate::observe::Observed;
use crate::tending::Tending;
use crate::{Error, Tools, lock, log, words};

/// What a send asks for.
pub(crate) struct Send {
    /// Where it was sent from, for the fleet whose seats `to` may name.
    pub(crate) directory: PathBuf,
    /// A seat's name, or a session's id.
    pub(crate) to: String,
    /// What to type in.
    pub(crate) text: String,
    /// Who sends it.
    pub(crate) sender: Sender,
}

/// Queues `ask`'s text for its recipient, with its step, or says why not.
/// The state lock is held from finding the recipient to queuing, so it
/// cannot go in between.
pub(crate) fn send(state: &Mutex<State>, tools: &Tools, ask: &Send) -> Result<Reply, Error> {
    if let Err(refusal) = messages::check_text(&ask.text) {
        return Ok(Reply::Status(refusal.into()));
    }
    let mut state = lock(state);
    let (to, fleet) = match recipient(&state, tools, ask)? {
        Ok(found) => found,
        Err(status) => return Ok(Reply::Status(status)),
    };
    let row = MessageRow {
        id: crate::new_id('m'),
        to: to.clone(),
        sender: ask.sender.clone(),
        text: ask.text.clone(),
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

/// Who `ask.to` is, and their fleet: a seat of the fleet here, else a
/// session whose pane is alive.
fn recipient(
    state: &State,
    tools: &Tools,
    ask: &Send,
) -> Result<Result<(Recipient, Option<PathBuf>), Status>, Error> {
    let resolved = names::resolve(state, &ask.directory, &ask.to, Takes::SeatOrSession)?;
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
pub(crate) fn deliver(
    state: &Mutex<State>,
    tools: &Tools,
    tending: &Tending,
    rows: &[SessionRow],
    observed: &Observed,
    now_millis: i64,
) {
    let held = match lock(state).held() {
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
        if !ids.is_empty() && idle && !blocked && type_in(state, tools, row, &ids) {
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
fn type_in(state: &Mutex<State>, tools: &Tools, row: &SessionRow, ids: &[String]) -> bool {
    let written = lock(state).write(|w| {
        let delivered = w.deliver(ids, |text| tools.mux.type_in(&row.pane, text))?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use fleet_core::protocol::response;

    use crate::tending::{self, Tending};
    use crate::testing::{FakeAgents, FakeMux, held, kill_all, steps, store, tools, with_seat};

    fn ask(directory: &Path, to: &str, text: &str) -> Send {
        Send {
            directory: directory.to_path_buf(),
            to: to.into(),
            text: text.into(),
            sender: Sender::Person,
        }
    }

    /// A seated fleet filled by one tick at 1 000: its root, state, fleet
    /// directory, tmux, and its seat's session id.
    fn filled(name: &str) -> (PathBuf, Mutex<State>, PathBuf, Arc<FakeMux>, String) {
        let (root, state, fleet) = with_seat(name);
        let mux = Arc::new(FakeMux::default());
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        tending::tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        let row = lock(&state).sessions().unwrap_or_default().remove(0);
        (root, state, fleet.directory, mux, row.id)
    }

    /// Who a send's reply says the message was queued for.
    fn queued(reply: Result<Reply, Error>) -> Option<Recipient> {
        match reply {
            Ok(Reply::Response(Response::Message(response::Message::Queued { message, to }))) => {
                assert!(message.starts_with('m') && message.len() == 11, "{message}");
                Some(to)
            }
            _ => None,
        }
    }

    /// Why a send was refused.
    fn refused(reply: Result<Reply, Error>) -> Option<Status> {
        match reply {
            Ok(Reply::Status(status)) => Some(status),
            _ => None,
        }
    }

    #[test]
    fn a_seat_here_is_queued_for_and_recorded() {
        let (_root, state, app, mux, _) = filled("msg-seat");
        let t = tools(mux, Arc::new(FakeAgents::default()));
        let src = app.join("src");
        let _ = std::fs::create_dir_all(&src);
        let to = queued(send(&state, &t, &ask(&src, "scribe", "hi")));
        assert_eq!(
            to,
            Some(Recipient::Seat {
                fleet: app.clone(),
                seat: "scribe".into()
            })
        );
        let held = held(&state);
        assert_eq!(held.len(), 1);
        assert_eq!(held[0].text, "hi");
        assert_eq!(held[0].sender, Sender::Person);
        assert_eq!(
            steps(&state).last().map(String::as_str),
            Some("MESSAGE_QUEUED")
        );
    }

    #[test]
    fn an_id_reaches_a_session_from_outside_every_fleet() {
        let (root, state, _, mux, id) = filled("msg-id");
        let t = tools(mux, Arc::new(FakeAgents::default()));
        let to = queued(send(&state, &t, &ask(&root.join("wing"), &id, "hi")));
        assert_eq!(to, Some(Recipient::Session { session: id }));
    }

    #[test]
    fn a_session_whose_pane_ended_is_refused_as_ended() {
        let (root, state, _, mux, id) = filled("msg-gone");
        kill_all(&mux);
        let t = tools(mux, Arc::new(FakeAgents::default()));
        assert_eq!(
            refused(send(&state, &t, &ask(&root, &id, "hi"))),
            Some(status::Message::SessionEnded { session: id }.into())
        );
        assert_eq!(held(&state), []);
    }

    #[test]
    fn a_tmux_that_cannot_list_refuses_a_session_message() {
        let (root, state, _, _, id) = filled("msg-notmux");
        let mux = Arc::new(FakeMux {
            missing: true,
            ..FakeMux::default()
        });
        let t = tools(mux, Arc::new(FakeAgents::default()));
        assert!(matches!(
            refused(send(&state, &t, &ask(&root, &id, "hi"))),
            Some(Status::Session(
                status::Session::MultiplexerCouldNotRun { .. }
            ))
        ));
    }

    #[test]
    fn text_that_breaks_the_rules_queues_nothing() {
        let (_root, state, app, mux, _) = filled("msg-text");
        let t = tools(mux, Arc::new(FakeAgents::default()));
        assert_eq!(
            refused(send(&state, &t, &ask(&app, "scribe", " \n"))),
            Some(status::Message::EmptyText.into())
        );
        let long = "a".repeat(messages::MAX_TEXT_BYTES + 1);
        assert!(matches!(
            refused(send(&state, &t, &ask(&app, "scribe", &long))),
            Some(Status::Message(status::Message::TooLong { .. }))
        ));
        assert_eq!(held(&state), []);
    }
}
