//! Watching (core.md § How a command flows, wait / events): both answer
//! by long-poll from the record. A poll reads under the state lock, and,
//! with nothing to say yet, waits on the state's bell without it, until a
//! step commits or its hold passes.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use fleet_core::agent::{SessionState, StateName};
use fleet_core::fleets;
use fleet_core::protocol::{
    BATCH_BYTES, Event, MAX_HOLD_MILLIS, Reply, Response, Waited, request, response,
};
use fleet_core::record::Step;
use fleet_core::status;
use fleet_core::waiting::{self, Verdict};
use state::{SessionRow, State};

use crate::{Error, lock};

/// `hold_millis`, at most the longest hold.
pub(crate) fn hold(hold_millis: u64) -> Duration {
    Duration::from_millis(hold_millis.min(MAX_HOLD_MILLIS))
}

/// The steps from `from` (the next one when none) about the fleet holding
/// `directory` (made canonical when it can be), with the supervisor's own,
/// or every step outside every fleet; held up to `hold` while there are
/// none.
pub(crate) fn events(
    state: &Mutex<State>,
    directory: &Path,
    from: Option<i64>,
    hold: Duration,
) -> Result<Reply, Error> {
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let (fleet, mut from, bell) = {
        let s = lock(state);
        let fleet = fleets::holding(&s.fleets()?, &directory).map(|f| f.directory.clone());
        let from = match from {
            Some(from) => from,
            None => s.next_id()?,
        };
        (fleet, from, s.bell())
    };
    let deadline = Instant::now() + hold;
    loop {
        let seen = bell.rung();
        let events = lock(state).events(from, fleet.as_deref(), BATCH_BYTES)?;
        let left = deadline.saturating_duration_since(Instant::now());
        if !events.is_empty() || left.is_zero() {
            if let Some(last) = events.last() {
                from = last.id + 1;
            }
            return Ok(Reply::Response(Response::Events(response::Events::Steps {
                fleet,
                events,
                next: from,
            })));
        }
        bell.wait_past(seen, left);
    }
}

/// One poll of a wait: the first resolves the name; a later one carries
/// what it resolved to.
pub(crate) fn wait(state: &Mutex<State>, ask: &request::Wait) -> Result<Reply, Error> {
    match ask {
        request::Wait::For {
            directory,
            name,
            states,
            hold_millis,
        } => {
            let first = first_poll(&lock(state), directory, name, states)?;
            match first {
                First::Decided(reply) => Ok(reply),
                First::From(on, from) => follow(state, &on, states, from, hold(*hold_millis)),
            }
        }
        request::Wait::On {
            on,
            states,
            from,
            hold_millis,
        } => follow(state, on, states, *from, hold(*hold_millis)),
    }
}

/// What a first poll found.
enum First {
    /// Answered already: reached, delivered, or refused.
    Decided(Reply),
    /// Not yet: follow what was resolved from this record id.
    From(Waited, i64),
}

/// What the first poll finds: who `name` is, and whether it is already
/// there. The caller holds the lock throughout, so the id it follows from
/// comes after everything it judged.
fn first_poll(
    s: &State,
    directory: &Path,
    name: &str,
    states: &[StateName],
) -> Result<First, Error> {
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let here = fleets::holding(&s.fleets()?, &directory).map(|f| f.directory.clone());
    let rows = s.sessions()?;
    let next = s.next_id()?;
    let refuse = |why: status::Wait| Ok(First::Decided(Reply::Status(why.into())));
    let Some(on) = resolve(s, here.as_ref(), &rows, name)? else {
        return refuse(status::Wait::NoSuchName {
            name: name.into(),
            fleet: here,
        });
    };
    if states.is_empty() && !matches!(on, Waited::Message { .. }) {
        return refuse(status::Wait::NoStates { name: name.into() });
    }
    let wanted = |st: SessionState| states.contains(&st.name());
    let now = match &on {
        Waited::Seat { fleet, seat } => rows
            .iter()
            .filter(|r| r.fleet.as_ref() == Some(fleet) && r.seat.as_ref() == Some(seat))
            .find_map(|r| {
                r.state.filter(|st| wanted(*st)).map(|st| Verdict::Reached {
                    session: r.id.clone(),
                    state: st,
                })
            })
            .unwrap_or(Verdict::Keep),
        Waited::Session { session } => {
            if let Some(r) = rows.iter().find(|r| r.id == *session) {
                match r.state {
                    Some(st) if wanted(st) => Verdict::Reached {
                        session: r.id.clone(),
                        state: st,
                    },
                    Some(SessionState::Gone) => Verdict::Never(status::Wait::Gone {
                        session: r.id.clone(),
                    }),
                    _ => Verdict::Keep,
                }
            } else {
                // No row: it was ended, or cleared as gone. Only what ended
                // it counts, never a state it had before.
                let endings: Vec<_> = s
                    .steps_with_session(session)?
                    .into_iter()
                    .filter(|e| ends(&e.step))
                    .collect();
                waiting::judge(&on, states, &endings)
            }
        }
        Waited::Message { message } => {
            let steps = s.steps_with_message(message)?;
            match waiting::judge(&on, states, &steps) {
                // Delivered, and a state wanted: the answer is its session's,
                // from the delivery on.
                Verdict::Delivered { session } if !states.is_empty() => {
                    let from = delivery(&steps, message).map_or(next, |id| id + 1);
                    return Ok(First::From(Waited::Session { session }, from));
                }
                verdict => verdict,
            }
        }
    };
    Ok(match now {
        Verdict::Keep => First::From(on, next),
        decided => First::Decided(answer(on, decided)),
    })
}

/// The id of the step that delivered `message`, among `events`.
fn delivery(events: &[Event], message: &str) -> Option<i64> {
    events
        .iter()
        .find(|e| matches!(&e.step, Step::MessageDelivered { message: m, .. } if m == message))
        .map(|e| e.id)
}

/// Whether `step` ends a session: ended, cleared as gone, or read gone.
fn ends(step: &Step) -> bool {
    matches!(
        step,
        Step::SessionEnded { .. }
            | Step::SessionGone { .. }
            | Step::SessionStateChanged {
                state: SessionState::Gone,
                ..
            }
    )
}

/// Who `name` is: a seat of the fleet `here`, else a session (live, or
/// launched in the record), else a message queued in the record.
fn resolve(
    s: &State,
    here: Option<&PathBuf>,
    rows: &[SessionRow],
    name: &str,
) -> Result<Option<Waited>, Error> {
    if let Some(fleet) = here
        && s.roster(fleet)?.seats.iter().any(|seat| seat.name == name)
    {
        return Ok(Some(Waited::Seat {
            fleet: fleet.clone(),
            seat: name.into(),
        }));
    }
    let launched = || -> Result<bool, Error> {
        Ok(s.steps_with_session(name)?
            .iter()
            .any(|e| matches!(e.step, Step::SessionLaunched { .. })))
    };
    if rows.iter().any(|r| r.id == name) || launched()? {
        return Ok(Some(Waited::Session {
            session: name.into(),
        }));
    }
    let queued = s
        .steps_with_message(name)?
        .iter()
        .any(|e| matches!(e.step, Step::MessageQueued { .. }));
    Ok(queued.then(|| Waited::Message {
        message: name.into(),
    }))
}

/// Judges the steps from `from` as they come, held up to `hold`; with
/// nothing decided by then, answers where to ask from next.
fn follow(
    state: &Mutex<State>,
    on: &Waited,
    states: &[StateName],
    mut from: i64,
    hold: Duration,
) -> Result<Reply, Error> {
    let mut on = on.clone();
    let bell = lock(state).bell();
    let deadline = Instant::now() + hold;
    loop {
        let seen = bell.rung();
        let events = lock(state).events(from, None, BATCH_BYTES)?;
        if let Some(last) = events.last() {
            from = last.id + 1;
        }
        match (waiting::judge(&on, states, &events), &on) {
            // Delivered, and a state wanted: follow its session from the
            // delivery on.
            (Verdict::Delivered { session }, Waited::Message { message }) if !states.is_empty() => {
                from = delivery(&events, message).map_or(from, |id| id + 1);
                on = Waited::Session { session };
                continue;
            }
            (Verdict::Keep, _) if !events.is_empty() => continue,
            (Verdict::Keep, _) => {}
            (decided, _) => return Ok(answer(on, decided)),
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            let (session, now) = live(&lock(state).sessions()?, &on);
            return Ok(Reply::Response(Response::Wait(response::Wait::Waiting {
                on,
                next: from,
                session,
                state: now,
            })));
        }
        bell.wait_past(seen, left);
    }
}

/// The reply a decided verdict makes.
fn answer(on: Waited, verdict: Verdict) -> Reply {
    match (verdict, on) {
        (Verdict::Reached { session, state }, on) => {
            Reply::Response(Response::Wait(response::Wait::Reached {
                on,
                session,
                state,
            }))
        }
        (Verdict::Delivered { session }, Waited::Message { message }) => {
            Reply::Response(Response::Wait(response::Wait::Delivered {
                message,
                session,
            }))
        }
        (Verdict::Never(why), _) => Reply::Status(why.into()),
        // judge delivers only messages, and never answers Keep here.
        (Verdict::Delivered { .. } | Verdict::Keep, on) => Reply::Status(
            status::Wait::NoSuchName {
                name: name_of(&on),
                fleet: None,
            }
            .into(),
        ),
    }
}

/// The name of what is waited on.
fn name_of(on: &Waited) -> String {
    match on {
        Waited::Seat { seat, .. } => seat.clone(),
        Waited::Session { session } => session.clone(),
        Waited::Message { message } => message.clone(),
    }
}

/// The live session of what is waited on, and its recorded state: a seat's
/// session that is not gone, else its newest; a session's own row; none
/// for a message.
fn live(rows: &[SessionRow], on: &Waited) -> (Option<String>, Option<SessionState>) {
    let pick = |r: &SessionRow| (Some(r.id.clone()), r.state);
    match on {
        Waited::Seat { fleet, seat } => {
            let mine: Vec<&SessionRow> = rows
                .iter()
                .filter(|r| r.fleet.as_ref() == Some(fleet) && r.seat.as_ref() == Some(seat))
                .collect();
            mine.iter()
                .find(|r| r.state != Some(SessionState::Gone))
                .or(mine.last())
                .map_or((None, None), |r| pick(r))
        }
        Waited::Session { session } => rows
            .iter()
            .find(|r| r.id == *session)
            .map_or((Some(session.clone()), None), pick),
        Waited::Message { .. } => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    use fleet_core::messages::{Recipient, Sender};

    use crate::testing::{FakeMux, code, put, spawned_row, with_seat};

    const NOW: Duration = Duration::ZERO;
    const SOON: Duration = Duration::from_millis(300);

    fn record(state: &Mutex<State>, step: &Step) {
        assert!(lock(state).write(|w| w.record(step)).is_ok());
    }

    fn seat_row(id: &str, fleet: &Path, state: Option<SessionState>) -> SessionRow {
        SessionRow {
            seat: Some("scribe".into()),
            state,
            ..spawned_row(id, fleet, 0)
        }
    }

    /// The ids of the events a reply carries; `[-1]` for any other reply,
    /// so an assertion on them fails showing it.
    fn steps_of(reply: &Reply) -> Vec<i64> {
        match reply {
            Reply::Response(Response::Events(response::Events::Steps { events, .. })) => {
                events.iter().map(|e| e.id).collect()
            }
            _ => vec![-1],
        }
    }

    fn path(p: &Path) -> String {
        p.display().to_string()
    }

    fn changed(fleet: &Path, session: &str, s: SessionState) -> Step {
        Step::SessionStateChanged {
            session: session.into(),
            fleet: Some(path(fleet)),
            seat: Some("scribe".into()),
            state: s,
        }
    }

    fn millis(d: Duration) -> u64 {
        u64::try_from(d.as_millis()).unwrap_or(0)
    }

    fn on_scribe(fleet: &Path, states: &[StateName], from: i64, hold: Duration) -> request::Wait {
        request::Wait::On {
            on: Waited::Seat {
                fleet: fleet.to_path_buf(),
                seat: "scribe".into(),
            },
            states: states.to_vec(),
            from,
            hold_millis: millis(hold),
        }
    }

    fn wait_for(
        state: &Mutex<State>,
        dir: &Path,
        name: &str,
        states: &[StateName],
        hold: Duration,
    ) -> Reply {
        let ask = request::Wait::For {
            directory: dir.to_path_buf(),
            name: name.into(),
            states: states.to_vec(),
            hold_millis: millis(hold),
        };
        wait(state, &ask).unwrap_or_else(|e| {
            Reply::Status(fleet_core::protocol::Status::Malformed {
                error: format!("{e:?}"),
            })
        })
    }

    fn queued(message: &str) -> Step {
        Step::MessageQueued {
            message: message.into(),
            to: Recipient::Session {
                session: "s1".into(),
            },
            sender: Sender::Script,
            text: "hi".into(),
            fleet: None,
        }
    }

    fn next_id(state: &Mutex<State>) -> i64 {
        lock(state).next_id().unwrap_or_default()
    }

    #[test]
    fn events_answer_at_once_with_what_is_there() {
        let (root, state, fleet) = with_seat("ev-now");
        let from = next_id(&state);
        let dir = path(&fleet.directory);
        record(
            &state,
            &Step::ProjectOn {
                project: dir.clone(),
                fleet: dir,
            },
        );
        let reply = events(
            &state,
            &code(&root, "app"),
            Some(from),
            Duration::from_secs(20),
        );
        assert_eq!(reply.as_ref().map(steps_of).ok(), Some(vec![from]));
    }

    #[test]
    fn events_from_now_hold_then_wake_on_a_commit() {
        let (root, state, fleet) = with_seat("ev-hold");
        let state = Arc::new(state);
        let started = Instant::now();
        let reply = thread::scope(|s| {
            let st = Arc::clone(&state);
            let dir = path(&fleet.directory);
            s.spawn(move || {
                thread::sleep(SOON);
                record(
                    &st,
                    &Step::ProjectOff {
                        project: dir.clone(),
                        fleet: dir,
                    },
                );
            });
            events(&state, &code(&root, "app"), None, Duration::from_secs(20))
        });
        assert_eq!(reply.as_ref().map(|r| steps_of(r).len()).ok(), Some(1));
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn events_end_empty_at_the_hold() {
        let (root, state, _) = with_seat("ev-empty");
        let reply = events(&state, &code(&root, "app"), None, SOON);
        assert_eq!(reply.as_ref().map(steps_of).ok(), Some(vec![]));
    }

    #[test]
    fn events_in_a_fleet_leave_out_another_fleets_steps() {
        let (root, state, fleet) = with_seat("ev-filter");
        let from = next_id(&state);
        record(
            &state,
            &Step::ProjectOn {
                project: "/x/api".into(),
                fleet: "/x/api".into(),
            },
        );
        record(&state, &Step::EveryFleetOff);
        let here = events(&state, &fleet.directory, Some(from), NOW);
        assert_eq!(here.as_ref().map(steps_of).ok(), Some(vec![from + 1]));
        let everywhere = events(&state, &root, Some(from), NOW);
        assert_eq!(
            everywhere.as_ref().map(steps_of).ok(),
            Some(vec![from, from + 1])
        );
    }

    #[test]
    fn a_wait_on_a_seat_already_idle_is_reached_at_once() {
        let (_, state, fleet) = with_seat("wait-now");
        put(
            &state,
            &FakeMux::default(),
            &seat_row("s1", &fleet.directory, Some(SessionState::Idle)),
        );
        let started = Instant::now();
        let reply = wait_for(
            &state,
            &fleet.directory,
            "scribe",
            &[StateName::Idle],
            Duration::from_secs(20),
        );
        assert!(
            matches!(&reply, Reply::Response(Response::Wait(response::Wait::Reached { session, .. })) if session == "s1"),
            "{reply:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_wait_not_yet_there_answers_waiting_with_where_to_ask_from() {
        let (_, state, fleet) = with_seat("wait-later");
        put(
            &state,
            &FakeMux::default(),
            &seat_row("s1", &fleet.directory, Some(SessionState::Working)),
        );
        let next = next_id(&state);
        let reply = wait_for(&state, &fleet.directory, "scribe", &[StateName::Idle], SOON);
        assert_eq!(
            reply,
            Reply::Response(Response::Wait(response::Wait::Waiting {
                on: Waited::Seat {
                    fleet: fleet.directory.clone(),
                    seat: "scribe".into()
                },
                next,
                session: Some("s1".into()),
                state: Some(SessionState::Working),
            }))
        );
    }

    #[test]
    fn a_state_that_came_and_went_since_the_cursor_is_reached() {
        let (_, state, fleet) = with_seat("wait-flip");
        let from = next_id(&state);
        record(&state, &changed(&fleet.directory, "s1", SessionState::Idle));
        record(
            &state,
            &changed(&fleet.directory, "s1", SessionState::Working),
        );
        let ask = on_scribe(&fleet.directory, &[StateName::Idle], from, NOW);
        assert!(matches!(
            wait(&state, &ask),
            Ok(Reply::Response(Response::Wait(
                response::Wait::Reached { .. }
            )))
        ));
    }

    #[test]
    fn a_held_wait_wakes_when_its_state_is_recorded() {
        let (_, state, fleet) = with_seat("wait-wake");
        let state = Arc::new(state);
        let from = next_id(&state);
        let started = Instant::now();
        let reply = thread::scope(|s| {
            let st = Arc::clone(&state);
            let dir = fleet.directory.clone();
            s.spawn(move || {
                thread::sleep(SOON);
                record(&st, &changed(&dir, "s1", SessionState::Idle));
            });
            wait(
                &state,
                &on_scribe(
                    &fleet.directory,
                    &[StateName::Idle],
                    from,
                    Duration::from_secs(20),
                ),
            )
        });
        assert!(
            matches!(
                reply,
                Ok(Reply::Response(Response::Wait(
                    response::Wait::Reached { .. }
                )))
            ),
            "{reply:?}"
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_wait_on_a_seat_follows_it_across_a_revival() {
        let (_, state, fleet) = with_seat("wait-revival");
        let from = next_id(&state);
        record(&state, &changed(&fleet.directory, "s1", SessionState::Gone));
        record(
            &state,
            &Step::SessionGone {
                session: "s1".into(),
                fleet: path(&fleet.directory),
                seat: "scribe".into(),
            },
        );
        record(&state, &changed(&fleet.directory, "s2", SessionState::Idle));
        let reply = wait(
            &state,
            &on_scribe(&fleet.directory, &[StateName::Idle], from, NOW),
        );
        assert!(
            matches!(&reply, Ok(Reply::Response(Response::Wait(response::Wait::Reached { session, .. }))) if session == "s2"),
            "{reply:?}"
        );
    }

    #[test]
    fn a_wait_on_a_gone_session_by_its_id_is_refused_at_once() {
        let (_, state, fleet) = with_seat("wait-gone");
        let gone = SessionRow {
            state: Some(SessionState::Gone),
            ..spawned_row("s1", &fleet.directory, 0)
        };
        put(&state, &FakeMux::default(), &gone);
        let reply = wait_for(&state, &fleet.directory, "s1", &[StateName::Idle], SOON);
        assert_eq!(
            reply,
            Reply::Status(
                status::Wait::Gone {
                    session: "s1".into()
                }
                .into()
            )
        );
    }

    #[test]
    fn a_wait_on_an_ended_session_is_refused() {
        let (_, state, fleet) = with_seat("wait-ended");
        record(
            &state,
            &Step::SessionLaunched {
                session: "s1".into(),
                adapter: "/a".into(),
                program: "/a".into(),
                directory: "/c".into(),
                model: None,
                prompt: None,
                fleet: None,
                seat: None,
                role: None,
            },
        );
        record(
            &state,
            &Step::SessionEnded {
                session: "s1".into(),
                fleet: None,
                seat: None,
            },
        );
        let reply = wait_for(&state, &fleet.directory, "s1", &[StateName::Idle], SOON);
        assert_eq!(
            reply,
            Reply::Status(
                status::Wait::Ended {
                    session: "s1".into()
                }
                .into()
            )
        );
    }

    #[test]
    fn an_ended_session_is_refused_though_it_was_idle_before() {
        let (_, state, fleet) = with_seat("wait-ended-idle");
        let launched = Step::SessionLaunched {
            session: "s1".into(),
            adapter: "/a".into(),
            program: "/a".into(),
            directory: "/c".into(),
            model: None,
            prompt: None,
            fleet: None,
            seat: None,
            role: None,
        };
        record(&state, &launched);
        record(
            &state,
            &Step::SessionStateChanged {
                session: "s1".into(),
                fleet: None,
                seat: None,
                state: SessionState::Idle,
            },
        );
        record(
            &state,
            &Step::SessionEnded {
                session: "s1".into(),
                fleet: None,
                seat: None,
            },
        );
        let reply = wait_for(&state, &fleet.directory, "s1", &[StateName::Idle], SOON);
        assert_eq!(
            reply,
            Reply::Status(
                status::Wait::Ended {
                    session: "s1".into()
                }
                .into()
            )
        );
    }

    #[test]
    fn a_message_is_waited_on_until_delivered() {
        let (_, state, fleet) = with_seat("wait-message");
        record(&state, &queued("m1"));
        let waiting = wait_for(&state, &fleet.directory, "m1", &[], NOW);
        assert!(
            matches!(
                waiting,
                Reply::Response(Response::Wait(response::Wait::Waiting { .. }))
            ),
            "{waiting:?}"
        );
        record(
            &state,
            &Step::MessageDelivered {
                message: "m1".into(),
                session: "s1".into(),
                fleet: None,
            },
        );
        assert_eq!(
            wait_for(&state, &fleet.directory, "m1", &[], NOW),
            Reply::Response(Response::Wait(response::Wait::Delivered {
                message: "m1".into(),
                session: "s1".into()
            }))
        );
    }

    #[test]
    fn a_message_dropped_is_refused() {
        let (_, state, fleet) = with_seat("wait-dropped");
        record(&state, &queued("m1"));
        record(
            &state,
            &Step::MessageDropped {
                message: "m1".into(),
                why: fleet_core::messages::Dropped::SessionEnded,
                fleet: None,
            },
        );
        assert_eq!(
            wait_for(&state, &fleet.directory, "m1", &[], NOW),
            Reply::Status(
                status::Wait::Dropped {
                    message: "m1".into(),
                    why: fleet_core::messages::Dropped::SessionEnded
                }
                .into()
            )
        );
    }

    #[test]
    fn a_name_nobody_has_is_refused_naming_the_fleet_searched() {
        let (root, state, fleet) = with_seat("wait-nobody");
        assert_eq!(
            wait_for(&state, &fleet.directory, "critic", &[StateName::Idle], NOW),
            Reply::Status(
                status::Wait::NoSuchName {
                    name: "critic".into(),
                    fleet: Some(fleet.directory.clone())
                }
                .into()
            )
        );
        assert_eq!(
            wait_for(&state, &root, "critic", &[StateName::Idle], NOW),
            Reply::Status(
                status::Wait::NoSuchName {
                    name: "critic".into(),
                    fleet: None
                }
                .into()
            )
        );
    }

    #[test]
    fn a_seat_or_session_needs_a_state() {
        let (_, state, fleet) = with_seat("wait-states");
        assert_eq!(
            wait_for(&state, &fleet.directory, "scribe", &[], NOW),
            Reply::Status(
                status::Wait::NoStates {
                    name: "scribe".into()
                }
                .into()
            )
        );
    }

    fn delivered(message: &str, session: &str) -> Step {
        Step::MessageDelivered {
            message: message.into(),
            session: session.into(),
            fleet: None,
        }
    }

    fn spawned_state(session: &str, s: SessionState) -> Step {
        Step::SessionStateChanged {
            session: session.into(),
            fleet: None,
            seat: None,
            state: s,
        }
    }

    #[test]
    fn a_message_with_a_state_waits_for_its_session_after_the_delivery() {
        let (_, state, fleet) = with_seat("wait-answer");
        record(&state, &queued("m1"));
        // Idle before the delivery is not the answer.
        record(&state, &spawned_state("s1", SessionState::Idle));
        let first = wait_for(&state, &fleet.directory, "m1", &[StateName::Idle], NOW);
        assert!(
            matches!(
                first,
                Reply::Response(Response::Wait(response::Wait::Waiting { .. }))
            ),
            "{first:?}"
        );
        record(&state, &delivered("m1", "s1"));
        record(&state, &spawned_state("s1", SessionState::Working));
        let mid = wait_for(&state, &fleet.directory, "m1", &[StateName::Idle], NOW);
        assert!(
            matches!(
                &mid,
                Reply::Response(Response::Wait(response::Wait::Waiting {
                    on: Waited::Session { session },
                    ..
                })) if session == "s1"
            ),
            "{mid:?}"
        );
        record(&state, &spawned_state("s1", SessionState::Idle));
        assert_eq!(
            wait_for(&state, &fleet.directory, "m1", &[StateName::Idle], NOW),
            Reply::Response(Response::Wait(response::Wait::Reached {
                on: Waited::Session {
                    session: "s1".into()
                },
                session: "s1".into(),
                state: SessionState::Idle,
            }))
        );
    }

    #[test]
    fn a_followed_message_turns_into_its_session_once_delivered() {
        let (_, state, _) = with_seat("wait-answer-follow");
        record(&state, &queued("m1"));
        let from = next_id(&state);
        record(&state, &spawned_state("s1", SessionState::Idle));
        record(&state, &delivered("m1", "s1"));
        record(&state, &spawned_state("s1", SessionState::Idle));
        let ask = request::Wait::On {
            on: Waited::Message {
                message: "m1".into(),
            },
            states: vec![StateName::Idle],
            from,
            hold_millis: 0,
        };
        let reply = wait(&state, &ask);
        assert!(
            matches!(
                &reply,
                Ok(Reply::Response(Response::Wait(response::Wait::Reached {
                    on: Waited::Session { .. },
                    ..
                })))
            ),
            "{reply:?}"
        );
    }

    #[test]
    fn a_seat_taken_out_while_waited_on_is_refused() {
        let (_, state, fleet) = with_seat("wait-removed");
        let from = next_id(&state);
        record(
            &state,
            &Step::SeatRemoved {
                fleet: path(&fleet.directory),
                seat: "scribe".into(),
            },
        );
        assert_eq!(
            wait(
                &state,
                &on_scribe(&fleet.directory, &[StateName::Idle], from, NOW)
            )
            .ok(),
            Some(Reply::Status(
                status::Wait::SeatRemoved {
                    fleet: fleet.directory.clone(),
                    seat: "scribe".into()
                }
                .into()
            ))
        );
    }

    #[test]
    fn a_poll_is_held_no_longer_than_the_longest_hold() {
        assert_eq!(
            hold(u64::MAX),
            Duration::from_millis(fleet_core::protocol::MAX_HOLD_MILLIS)
        );
        assert_eq!(hold(1_500), Duration::from_millis(1_500));
    }
}
