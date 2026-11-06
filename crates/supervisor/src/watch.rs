//! Watching (core.md § How a command flows, wait / events): both answer
//! by long-poll from the record. A poll reads under the state lock, and,
//! with nothing to say yet, waits on the state's bell without it, until a
//! step commits or its hold passes.

use std::path::Path;
use std::time::{Duration, Instant};

use fleet_core::agent::{SessionState, StateName};
use fleet_core::protocol::{
    BATCH_BYTES, MAX_HOLD_MILLIS, Reply, Response, Status, Waited, request, response,
};
use fleet_core::record::Step;
use fleet_core::seats::SeatKey;
use fleet_core::status::{self, Takes};
use fleet_core::waiting::{self, Verdict};
use state::{SessionRow, State};

use crate::names::{self, Named};
use crate::{Error, Supervisor};

/// `hold_millis`, at most the longest hold.
fn hold(hold_millis: u64) -> Duration {
    Duration::from_millis(hold_millis.min(MAX_HOLD_MILLIS))
}

/// The steps from `from` (the next one when none) about the fleet holding
/// `directory`, with the supervisor's own,
/// or every step outside every fleet; held up to `hold_millis` while there
/// are none.
pub(crate) fn events(sup: &Supervisor, ask: request::Events) -> Result<Reply, Error> {
    let request::Events::Follow {
        directory,
        from,
        hold_millis,
    } = ask;
    let (fleet, mut from, bell) = {
        let s = sup.lock();
        let fleet = names::here(&s, &directory)?.map(|f| f.directory);
        let from = match from {
            Some(from) => from,
            None => s.next_id()?,
        };
        (fleet, from, s.bell())
    };
    let deadline = Instant::now() + hold(hold_millis);
    loop {
        let seen = bell.rung();
        let events = sup.lock().events(from, fleet.as_deref(), BATCH_BYTES)?;
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
pub(crate) fn wait(sup: &Supervisor, ask: &request::Wait) -> Result<Reply, Error> {
    match ask {
        request::Wait::For {
            directory,
            name,
            states,
            hold_millis,
        } => {
            let first = first_poll(&sup.lock(), directory, name, states)?;
            match first {
                First::Decided(reply) => Ok(reply),
                First::From(on, from) => follow(sup, &on, states, from, hold(*hold_millis)),
            }
        }
        request::Wait::On {
            on,
            states,
            from,
            hold_millis,
        } => follow(sup, on, states, *from, hold(*hold_millis)),
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
/// comes after everything it judged. A message is followed from its
/// queuing, which its own steps come after.
fn first_poll(
    s: &State,
    directory: &Path,
    name: &str,
    states: &[StateName],
) -> Result<First, Error> {
    let next = s.next_id()?;
    let refuse = |why: Status| Ok(First::Decided(Reply::Status(why)));
    let resolved = names::resolve(s, directory, name, Takes::SeatSessionOrMessage)?;
    let Some(named) = resolved.named else {
        return refuse(resolved.unknown().into());
    };
    if states.is_empty() && !matches!(named, Named::Message(_)) {
        return refuse(status::Wait::NoStates { name: name.into() }.into());
    }
    // Each session's state as last recorded, judged as its step would be.
    let already = |on: Waited, rows: Vec<SessionRow>| {
        let recorded: Vec<(String, SessionState)> = rows
            .into_iter()
            .filter_map(|r| Some((r.id, r.state?)))
            .collect();
        let now = waiting::already(&on, states, &recorded);
        (on, now)
    };
    let (on, now) = match named {
        Named::Seat(SeatKey { fleet, seat }) => {
            let mine = s
                .sessions()?
                .into_iter()
                .filter(|r| names::of_seat(r, &fleet, &seat))
                .collect();
            already(Waited::Seat { fleet, seat }, mine)
        }
        Named::Session(row) => already(
            Waited::Session {
                session: row.id.clone(),
            },
            vec![row],
        ),
        Named::Ended(session) => {
            // Ended, or cleared as gone: only what ended it counts, never a
            // state it had before.
            let endings: Vec<_> = s
                .steps_with_session(&session)?
                .into_iter()
                .filter(|e| ends(&e.step))
                .collect();
            waiting::judge(&Waited::Session { session }, states, &endings)
        }
        Named::Message(message) => {
            let queued = s.steps_with_message(&message)?.first().map(|e| e.id);
            return Ok(First::From(
                Waited::Message { message },
                queued.unwrap_or(next),
            ));
        }
    };
    Ok(match now {
        Verdict::Keep => First::From(on, next),
        decided => First::Decided(answer(on, decided)),
    })
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

/// Judges the steps from `from` as they come, held up to `hold`; with
/// nothing decided by then, answers where to ask from next.
fn follow(
    sup: &Supervisor,
    on: &Waited,
    states: &[StateName],
    mut from: i64,
    hold: Duration,
) -> Result<Reply, Error> {
    let mut on = on.clone();
    let bell = sup.lock().bell();
    let deadline = Instant::now() + hold;
    loop {
        let seen = bell.rung();
        let events = sup.lock().events(from, None, BATCH_BYTES)?;
        if let Some(last) = events.last() {
            from = last.id + 1;
        }
        let (now_on, verdict) = waiting::judge(&on, states, &events);
        on = now_on;
        match verdict {
            Verdict::Keep if !events.is_empty() => continue,
            Verdict::Keep => {}
            decided => return Ok(answer(on, decided)),
        }
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            let (session, now) = live(&sup.lock().sessions()?, &on);
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
        // judge delivers only messages, and Keep is never answered.
        (Verdict::Delivered { .. } | Verdict::Keep, on) => Reply::Status(
            status::Naming::Unknown {
                name: name_of(&on),
                fleet: None,
                takes: Takes::SeatSessionOrMessage,
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
                .filter(|r| names::of_seat(r, fleet, seat))
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

    #[test]
    fn a_poll_is_held_no_longer_than_the_longest_hold() {
        assert_eq!(
            hold(u64::MAX),
            Duration::from_millis(fleet_core::protocol::MAX_HOLD_MILLIS)
        );
        assert_eq!(hold(1_500), Duration::from_millis(1_500));
    }
}
