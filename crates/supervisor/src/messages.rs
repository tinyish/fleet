//! Messages (core.md § Primitives, Messages): sending one queues it for a
//! seat of the fleet here or for a session by its id; delivering, which
//! the tending loop does each tick, types every message waiting for an idle
//! agent in as one paste, then holds that session until it has left idle or
//! 30 s pass. A message that can no longer reach anyone is dropped, and
//! recorded, in the change that ends its session, seat or fleet.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::agent::SessionState;
use fleet_core::fleets;
use fleet_core::messages::{self, Dropped, Hold, Recipient, Sender};
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::status;
use state::{MessageRow, SessionRow, State, StateError, Step, Write, now_millis};

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
    let to = match recipient(&state, tools, ask)? {
        Ok(to) => to,
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

/// Who `ask.to` is: a seat of the fleet holding `ask.directory` (made
/// canonical when it can be), else a session with that id whose pane is
/// alive.
fn recipient(state: &State, tools: &Tools, ask: &Send) -> Result<Result<Recipient, Status>, Error> {
    let directory = std::fs::canonicalize(&ask.directory).unwrap_or_else(|_| ask.directory.clone());
    let fleets = state.fleets()?;
    let here = fleets::holding(&fleets, &directory).map(|f| f.directory.clone());
    if let Some(fleet) = &here
        && state.roster(fleet)?.seats.iter().any(|s| s.name == ask.to)
    {
        return Ok(Ok(Recipient::Seat {
            fleet: fleet.clone(),
            seat: ask.to.clone(),
        }));
    }
    let Some(row) = state.sessions()?.into_iter().find(|r| r.id == ask.to) else {
        return Ok(Err(status::Message::NoSuchRecipient {
            to: ask.to.clone(),
            fleet: here,
        }
        .into()));
    };
    Ok(
        match crate::observe::alive(tools, std::slice::from_ref(&row)) {
            Ok(alive) if alive.get(&row.id).copied().unwrap_or(false) => {
                Ok(Recipient::Session { session: row.id })
            }
            Ok(_) => Err(status::Message::SessionGone { session: row.id }.into()),
            Err(e) => Err(crate::sessions::tool_status(e)),
        },
    )
}

/// Delivers what waits for each idle agent at `now_millis`: one paste per
/// session, then a hold (core.md invariant 12). Only the sessions with
/// messages waiting, and those held, are read; nothing is typed when tmux
/// or the adapter cannot say the agent is idle.
pub(crate) fn deliver(state: &Mutex<State>, tools: &Tools, tending: &Tending, now_millis: i64) {
    let read = {
        let s = lock(state);
        s.held().and_then(|h| s.sessions().map(|r| (h, r)))
    };
    let (held, rows) = match read {
        Ok(read) => read,
        Err(e) => return log(&words::tending_failed(&e.into())),
    };
    let mut holds = tending.holds();
    holds.retain(|session, hold| hold.blocks(now_millis) && rows.iter().any(|r| r.id == *session));
    if held.is_empty() && holds.is_empty() {
        return tending.set_holds(holds);
    }
    let asked: Vec<SessionRow> = rows
        .into_iter()
        .filter(|r| holds.contains_key(&r.id) || !waiting_for(&held, r).is_empty())
        .collect();
    let Ok(observed) = crate::observe::observe(tools, &asked, now_millis) else {
        return tending.set_holds(holds);
    };
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
            Recipient::Seat { fleet, seat } => {
                row.fleet.as_ref() == Some(fleet) && row.seat.as_ref() == Some(seat)
            }
        })
        .map(|m| m.id.clone())
        .collect()
}

/// Types the messages `ids` still held into `row`'s pane as one paste, with
/// a step for each, in one transaction. Whether any went in.
fn type_in(state: &Mutex<State>, tools: &Tools, row: &SessionRow, ids: &[String]) -> bool {
    let written = lock(state).write(|w| {
        let delivered = w.deliver(ids, |text| tools.mux.type_in(&row.pane, text))?;
        for message in &delivered {
            w.record(&Step::MessageDelivered {
                message: message.clone(),
                session: row.id.clone(),
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
/// transaction: the one whose change dropped them.
pub(crate) fn dropped(w: &mut Write<'_>, ids: &[String], why: Dropped) -> Result<(), StateError> {
    ids.iter().try_for_each(|message| {
        w.record(&Step::MessageDropped {
            message: message.clone(),
            why,
        })
    })
}

/// Drops, and records, every message held for a seat of the fleet in
/// `fleet`, which is leaving: removed, or forgotten.
pub(crate) fn fleet_leaves(w: &mut Write<'_>, fleet: &Path) -> Result<(), StateError> {
    let ids = w.drop_for_fleet(fleet)?;
    dropped(w, &ids, Dropped::FleetRemoved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, PoisonError};

    use fleet_core::agent::{Activity, Reading};
    use fleet_core::protocol::response;
    use std::collections::BTreeMap;

    use crate::adapters::Store;
    use crate::tending::{self, Tending};
    use crate::testing::{FakeAgents, FakeMux, FakeNet, Shared, SharedMux, with_seat};

    fn tools(mux: Arc<FakeMux>, agents: Arc<FakeAgents>) -> Tools {
        Tools {
            mux: Box::new(SharedMux(mux)),
            agents: Box::new(Shared(agents)),
            net: Box::new(FakeNet::default()),
        }
    }

    fn store(root: &Path) -> Store {
        Store::new(&root.join("st"), Some("https://r".into()))
    }

    fn ask(directory: &Path, to: &str, text: &str) -> Send {
        Send {
            directory: directory.to_path_buf(),
            to: to.into(),
            text: text.into(),
            sender: Sender::Person,
        }
    }

    fn steps(state: &Mutex<State>) -> Vec<String> {
        lock(state).steps().unwrap_or_default()
    }

    fn held(state: &Mutex<State>) -> Vec<MessageRow> {
        lock(state).held().unwrap_or_default()
    }

    fn kill_all(mux: &FakeMux) {
        for p in mux
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
        {
            p.dead = true;
        }
    }

    /// A seated fleet filled by one tick at 1 000: its root, state, fleet
    /// directory, tmux, and its seat's session id and pane.
    fn filled(name: &str) -> (PathBuf, Mutex<State>, PathBuf, Arc<FakeMux>, String, String) {
        let (root, state, fleet) = with_seat(name);
        let mux = Arc::new(FakeMux::default());
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        tending::tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        let row = lock(&state).sessions().unwrap_or_default().remove(0);
        (root, state, fleet.directory, mux, row.id, row.pane)
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
        let (_root, state, app, mux, _, _) = filled("msg-seat");
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
        let (root, state, _, mux, id, _) = filled("msg-id");
        let t = tools(mux, Arc::new(FakeAgents::default()));
        let to = queued(send(&state, &t, &ask(&root.join("wing"), &id, "hi")));
        assert_eq!(to, Some(Recipient::Session { session: id }));
    }

    #[test]
    fn a_seat_wins_over_a_session_of_the_same_name() {
        let (_root, state, app, mux, _, _) = filled("msg-shadow");
        let row = state::SessionRow {
            id: "scribe".into(),
            adapter: "/a/claude".into(),
            directory: app.clone(),
            launch_time_millis: 1,
            pane: String::new(),
            fleet: None,
            seat: None,
            role: None,
            role_digest: None,
        };
        let launched = lock(&state).write(|w| w.launch_session(&row, || Ok("%9".into())));
        assert!(launched.is_ok());
        let t = tools(mux, Arc::new(FakeAgents::default()));
        assert!(matches!(
            queued(send(&state, &t, &ask(&app, "scribe", "hi"))),
            Some(Recipient::Seat { .. })
        ));
    }

    #[test]
    fn nobody_by_that_name_is_refused_naming_the_fleet_searched() {
        let (root, state, app, mux, _, _) = filled("msg-nobody");
        let t = tools(mux, Arc::new(FakeAgents::default()));
        assert_eq!(
            refused(send(&state, &t, &ask(&app, "critic", "hi"))),
            Some(
                status::Message::NoSuchRecipient {
                    to: "critic".into(),
                    fleet: Some(app)
                }
                .into()
            )
        );
        assert_eq!(
            refused(send(&state, &t, &ask(&root.join("wing"), "scribe", "hi"))),
            Some(
                status::Message::NoSuchRecipient {
                    to: "scribe".into(),
                    fleet: None
                }
                .into()
            )
        );
        assert_eq!(held(&state), []);
    }

    #[test]
    fn a_session_already_gone_is_refused() {
        let (root, state, _, mux, id, _) = filled("msg-gone");
        kill_all(&mux);
        let t = tools(mux, Arc::new(FakeAgents::default()));
        assert_eq!(
            refused(send(&state, &t, &ask(&root, &id, "hi"))),
            Some(status::Message::SessionGone { session: id }.into())
        );
        assert_eq!(held(&state), []);
    }

    #[test]
    fn a_tmux_that_cannot_list_refuses_a_session_message() {
        let (root, state, _, _, id, _) = filled("msg-notmux");
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
        let (_root, state, app, mux, _, _) = filled("msg-text");
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

    /// Adapters reading `session` of `/a/claude` as `activity`.
    fn reading(session: &str, activity: Activity) -> Arc<FakeAgents> {
        Arc::new(FakeAgents {
            readings: Some(BTreeMap::from([(
                PathBuf::from("/a/claude"),
                vec![Reading {
                    session: session.into(),
                    activity,
                    reason: None,
                }],
            )])),
            ..FakeAgents::default()
        })
    }

    fn typed(mux: &FakeMux) -> Vec<(String, String)> {
        mux.typed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Queues `text` for `scribe`, sent from `app`.
    fn sent(state: &Mutex<State>, mux: &Arc<FakeMux>, app: &Path, text: &str) {
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        assert!(queued(send(state, &t, &ask(app, "scribe", text))).is_some());
    }

    #[test]
    fn an_idle_agent_gets_every_waiting_message_in_one_paste() {
        let (_root, state, app, mux, id, pane) = filled("dlv-idle");
        sent(&state, &mux, &app, "one\n");
        sent(&state, &mux, &app, "two");
        let t = tools(mux.clone(), reading(&id, Activity::Idle));
        deliver(&state, &t, &Tending::new(), 60_000);
        assert_eq!(typed(&mux), [(pane, "one\n\ntwo".to_owned())]);
        assert_eq!(held(&state), []);
        let delivered = steps(&state)
            .iter()
            .filter(|s| *s == "MESSAGE_DELIVERED")
            .count();
        assert_eq!(delivered, 2);
    }

    #[test]
    fn nothing_is_typed_unless_the_agent_reads_idle() {
        let (_root, state, app, mux, id, _) = filled("dlv-busy");
        sent(&state, &mux, &app, "hi");
        for activity in [
            Activity::Working,
            Activity::NeedsYou,
            Activity::Starting,
            Activity::Unknown,
        ] {
            let t = tools(mux.clone(), reading(&id, activity));
            deliver(&state, &t, &Tending::new(), 2_000);
        }
        let unread = Arc::new(FakeAgents {
            readings: Some(BTreeMap::new()),
            ..FakeAgents::default()
        });
        deliver(&state, &tools(mux.clone(), unread), &Tending::new(), 2_000);
        assert_eq!(typed(&mux), []);
        assert_eq!(held(&state).len(), 1);
    }

    #[test]
    fn after_a_paste_the_session_is_held_until_it_leaves_idle() {
        let (_root, state, app, mux, id, _) = filled("dlv-hold");
        let tending = Tending::new();
        let idle = || tools(mux.clone(), reading(&id, Activity::Idle));
        sent(&state, &mux, &app, "first");
        deliver(&state, &idle(), &tending, 10_000);
        sent(&state, &mux, &app, "second");
        deliver(&state, &idle(), &tending, 12_000);
        assert_eq!(typed(&mux).len(), 1, "still idle from before the paste");
        let working = tools(mux.clone(), reading(&id, Activity::Working));
        deliver(&state, &working, &tending, 14_000);
        assert_eq!(typed(&mux).len(), 1, "working: nothing typed");
        deliver(&state, &idle(), &tending, 16_000);
        assert_eq!(typed(&mux).last().map(|t| t.1.as_str()), Some("second"));
    }

    #[test]
    fn a_hold_ends_after_thirty_seconds_still_idle() {
        let (_root, state, app, mux, id, _) = filled("dlv-expire");
        let tending = Tending::new();
        let idle = || tools(mux.clone(), reading(&id, Activity::Idle));
        sent(&state, &mux, &app, "first");
        deliver(&state, &idle(), &tending, 10_000);
        sent(&state, &mux, &app, "second");
        deliver(
            &state,
            &idle(),
            &tending,
            10_000 + messages::HOLD_MILLIS - 1,
        );
        assert_eq!(typed(&mux).len(), 1);
        deliver(&state, &idle(), &tending, 10_000 + messages::HOLD_MILLIS);
        assert_eq!(typed(&mux).len(), 2);
    }

    #[test]
    fn held_messages_survive_a_restart() {
        let (_root, state, app, mux, id, _) = filled("dlv-restart");
        sent(&state, &mux, &app, "hi");
        // A fresh Tending is a restarted supervisor: the same state.
        let t = tools(mux.clone(), reading(&id, Activity::Idle));
        deliver(&state, &t, &Tending::new(), 5_000);
        assert_eq!(typed(&mux).len(), 1);
    }

    #[test]
    fn a_seat_message_waits_for_the_revived_session() {
        let (root, state, app, mux, old, _) = filled("dlv-revive");
        kill_all(&mux);
        sent(&state, &mux, &app, "hi");
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        deliver(&state, &t, &Tending::new(), 2_000);
        assert_eq!(typed(&mux), [], "no live session: held");
        assert_eq!(held(&state).len(), 1, "not dropped either");
        // The next tick revives the seat, then delivers to the new session.
        tending::tick(&state, &t, &store(&root), &Tending::new(), 3_000);
        let new = lock(&state).sessions().unwrap_or_default().remove(0);
        assert_ne!(new.id, old);
        assert_eq!(typed(&mux), [(new.pane, "hi".to_owned())]);
    }

    #[test]
    fn a_hold_starts_when_the_paste_goes_in_not_when_the_tick_began() {
        let (root, state, app, _, _, _) = filled("dlv-late");
        // The tick's revival takes 300 ms before delivery runs.
        let mux = Arc::new(FakeMux {
            launch_delay: std::time::Duration::from_millis(300),
            ..FakeMux::default()
        });
        sent(&state, &mux, &app, "hi");
        let tending = Tending::new();
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        tending::tick(&state, &t, &store(&root), &tending, 10_000);
        assert_eq!(typed(&mux).len(), 1);
        let since = tending.holds().values().next().map(|h| h.since_millis);
        assert!(since.is_some_and(|s| s >= 10_300), "{since:?}");
    }

    #[test]
    fn an_off_fleets_live_seat_still_gets_its_message() {
        let (_root, state, app, mux, id, _) = filled("dlv-off");
        assert!(lock(&state).write(|w| w.set_all_active(false)).is_ok());
        sent(&state, &mux, &app, "hi");
        let t = tools(mux.clone(), reading(&id, Activity::Idle));
        deliver(&state, &t, &Tending::new(), 2_000);
        assert_eq!(typed(&mux).len(), 1);
    }

    #[test]
    fn a_paste_that_fails_keeps_the_messages_for_the_next_tick() {
        let (_root, state, app, mux, id, _) = filled("dlv-fail");
        sent(&state, &mux, &app, "hi");
        let panes = mux
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let failing = Arc::new(FakeMux {
            panes: Mutex::new(panes),
            fail_type: true,
            ..FakeMux::default()
        });
        let tending = Tending::new();
        deliver(
            &state,
            &tools(failing, reading(&id, Activity::Idle)),
            &tending,
            2_000,
        );
        assert_eq!(held(&state).len(), 1);
        assert!(!steps(&state).iter().any(|s| s == "MESSAGE_DELIVERED"));
        let t = tools(mux.clone(), reading(&id, Activity::Idle));
        deliver(&state, &t, &tending, 2_100);
        assert_eq!(typed(&mux).len(), 1, "no hold after a failed paste");
    }

    /// Queues a message for session `id`, sent from `from`.
    fn sent_to(state: &Mutex<State>, mux: &Arc<FakeMux>, from: &Path, id: &str) {
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        assert!(queued(send(state, &t, &ask(from, id, "hi"))).is_some());
    }

    /// Whether `MESSAGE_DROPPED` was recorded after the last `after`.
    fn dropped_after(state: &Mutex<State>, after: &str) -> bool {
        let s = steps(state);
        let at = s.iter().rposition(|x| x == after);
        let dropped = s.iter().rposition(|x| x == "MESSAGE_DROPPED");
        at.is_some() && dropped > at
    }

    #[test]
    fn ending_a_session_drops_its_messages() {
        let (root, state, _, mux, id, _) = filled("drop-stop");
        sent_to(&state, &mux, &root, &id);
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        assert!(crate::sessions::end(&state, &t, &root, &id).is_ok());
        assert_eq!(held(&state), []);
        assert!(
            dropped_after(&state, "SESSION_ENDED"),
            "{:?}",
            steps(&state)
        );
    }

    #[test]
    fn ending_a_seats_session_keeps_the_seats_messages() {
        let (_root, state, app, mux, id, _) = filled("end-keeps-seat");
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        assert!(queued(send(&state, &t, &ask(&app, "scribe", "for the seat"))).is_some());
        assert!(queued(send(&state, &t, &ask(&app, &id, "for the session"))).is_some());
        assert!(crate::sessions::end(&state, &t, &app, "scribe").is_ok());
        let left: Vec<String> = held(&state).into_iter().map(|m| m.text).collect();
        assert_eq!(left, ["for the seat"]);
    }

    #[test]
    fn a_seat_wins_over_a_session_of_the_same_name_for_end() {
        let (_root, state, app, mux, id, _) = filled("end-shadow");
        let row = state::SessionRow {
            id: "scribe".into(),
            ..crate::testing::spawned_row("x", Path::new("/gone"), 1)
        };
        crate::testing::put(&state, &SharedMux(mux.clone()), &row);
        let t = tools(mux, Arc::new(FakeAgents::default()));
        assert!(matches!(
            crate::sessions::end(&state, &t, &app, "scribe"),
            Ok(Reply::Response(Response::End(response::End::Ended { session, seat: Some(_) }))) if session == id
        ));
    }

    #[test]
    fn a_gone_seat_sessions_messages_drop_at_its_revival() {
        let (root, state, _, mux, id, _) = filled("drop-gone");
        sent_to(&state, &mux, &root, &id);
        kill_all(&mux);
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        tending::tick(&state, &t, &store(&root), &Tending::new(), 3_000);
        assert_eq!(held(&state), []);
        assert!(dropped_after(&state, "SESSION_GONE"), "{:?}", steps(&state));
        assert_eq!(
            typed(&mux),
            [],
            "a message to a session never follows its seat"
        );
    }

    #[test]
    fn a_seat_taken_out_drops_its_messages() {
        let (_root, state, app, mux, _, _) = filled("drop-seat");
        sent(&state, &mux, &app, "hi");
        let old = lock(&state).roster(&app).unwrap_or_default();
        let new = fleet_core::seats::Roster {
            seats: vec![],
            ..old.clone()
        };
        let moved = fleet_core::seats::diff(&old, &new);
        let written = lock(&state).write(|w| crate::projects::set_roster(w, &app, &new, &moved));
        assert!(written.is_ok());
        assert_eq!(held(&state), []);
        assert!(dropped_after(&state, "SEAT_REMOVED"), "{:?}", steps(&state));
    }

    #[test]
    fn removing_the_fleet_drops_its_seats_messages() {
        let (_root, state, app, mux, _, _) = filled("drop-remove");
        sent(&state, &mux, &app, "hi");
        let removed = crate::remove::remove(
            &state,
            &app,
            &[],
            fleet_core::protocol::request::ConfigChoice::Keep,
        );
        assert!(removed.is_ok(), "{removed:?}");
        assert_eq!(held(&state), []);
        assert!(
            dropped_after(&state, "FLEET_REMOVED"),
            "{:?}",
            steps(&state)
        );
    }

    #[test]
    fn a_forgotten_fleet_drops_its_seats_messages() {
        let (_root, state, app, mux, _, _) = filled("drop-forget");
        sent(&state, &mux, &app, "hi");
        let _ = std::fs::remove_dir_all(&app);
        assert!(crate::projects::all_on(&state).is_ok());
        assert_eq!(held(&state), []);
        assert!(
            dropped_after(&state, "FLEET_FORGOTTEN"),
            "{:?}",
            steps(&state)
        );
    }
}
