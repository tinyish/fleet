//! What is typed in, and when: messages sent, held and delivered through
//! the paste hold, and dropped with what ends their session or seat.

use std::sync::atomic::Ordering;
use std::time::Duration;

use fleet_core::agent::{Activity, SessionState, StateName};
use fleet_core::messages::{Dropped, HOLD_MILLIS};
use fleet_core::protocol::{Reply, Request, Response, Waited, request, response};
use fleet_core::status;

use super::{next_of, reached, refused, rig, rig_on, waiting_on};
use crate::testing::{FakeMux, held, kill_all, steps, typed};

#[test]
fn a_message_is_typed_in_at_the_next_idle_tick_and_its_wait_ends_delivered() {
    let r = rig("sc-deliver");
    r.tick(1_000);
    let one = r.send("scribe", "one\n");
    let two = r.send("scribe", "two");
    let before = r.wait(&one, &[]);
    assert!(
        matches!(waiting_on(&before), Some(Waited::Message { .. })),
        "{before:?}"
    );
    r.tick(2_000);
    let s = r.seated();
    assert_eq!(typed(&r.mux), [(s.pane.clone(), "one\n\ntwo".to_owned())]);
    assert_eq!(held(&r.sup.state), []);
    for m in [one, two] {
        assert_eq!(
            r.wait(&m, &[]),
            Reply::Response(Response::Wait(response::Wait::Delivered {
                message: m.clone(),
                session: s.id.clone()
            }))
        );
    }
}

#[test]
fn a_wait_for_the_answer_ends_once_the_agent_left_idle_and_is_idle_again() {
    let r = rig("sc-answer");
    r.tick(1_000);
    let m = r.send("scribe", "first");
    let first = r.wait(&m, &[StateName::Idle]);
    assert!(
        matches!(waiting_on(&first), Some(Waited::Message { .. })),
        "{first:?}"
    );
    r.tick(2_000);
    let s = r.seated().id;
    let session = Waited::Session { session: s.clone() };
    // Still idle from before the paste: not the answer, and no second
    // paste.
    let second = r.send("scribe", "second");
    r.tick(2_500);
    let mid = r.wait(&m, &[StateName::Idle]);
    assert_eq!(waiting_on(&mid), Some(&session), "{mid:?}");
    assert_eq!(typed(&r.mux).len(), 1);
    r.agents.read_as(Activity::Working);
    r.tick(3_000);
    assert_eq!(typed(&r.mux).len(), 1, "working: nothing typed");
    r.agents.read_as(Activity::Idle);
    r.tick(4_000);
    let answered = reached(session.clone(), &s, SessionState::Idle);
    assert_eq!(r.wait(&m, &[StateName::Idle]), answered);
    let followed = Waited::Message { message: m };
    assert_eq!(
        r.wait_on(followed, &[StateName::Idle], next_of(&first)),
        answered
    );
    assert_eq!(typed(&r.mux).last().map(|t| t.1.as_str()), Some("second"));
    assert!(matches!(
        r.wait(&second, &[]),
        Reply::Response(Response::Wait(response::Wait::Delivered { .. }))
    ));
}

#[test]
fn an_agent_still_idle_after_a_paste_answers_once_the_hold_has_passed() {
    let r = rig("sc-hold");
    r.tick(1_000);
    let m = r.send("scribe", "first");
    r.tick(10_000);
    let _ = r.send("scribe", "second");
    r.tick(10_000 + HOLD_MILLIS - 1_000);
    assert_eq!(typed(&r.mux).len(), 1, "held");
    assert!(waiting_on(&r.wait(&m, &[StateName::Idle])).is_some());
    r.tick(10_000 + HOLD_MILLIS + 1_000);
    assert_eq!(typed(&r.mux).len(), 2, "the hold has passed");
    assert!(matches!(
        r.wait(&m, &[StateName::Idle]),
        Reply::Response(Response::Wait(response::Wait::Reached { .. }))
    ));
}

#[test]
fn a_hold_starts_when_the_paste_goes_in_not_when_the_tick_began() {
    // The tick's revival takes 300 ms before delivery runs.
    let r = rig_on(
        "sc-late",
        FakeMux {
            launch_delay: Duration::from_millis(300),
            ..FakeMux::default()
        },
    );
    r.tick(1_000);
    let _ = r.send("scribe", "hi");
    kill_all(&r.mux);
    r.tick(10_000);
    assert_eq!(typed(&r.mux).len(), 1);
    let since = r
        .sup
        .tending
        .holds()
        .values()
        .next()
        .map(|h| h.since_millis);
    assert!(since.is_some_and(|s| s >= 10_300), "{since:?}");
}

#[test]
fn nothing_is_typed_while_the_agent_reads_anything_but_idle() {
    let r = rig("sc-busy");
    r.tick(1_000);
    let _ = r.send("scribe", "hi");
    for (now, activity) in [
        (2_000, Activity::Working),
        (3_000, Activity::NeedsYou),
        (4_000, Activity::Starting),
        (5_000, Activity::Unknown),
    ] {
        r.agents.read_as(activity);
        r.tick(now);
    }
    assert_eq!(typed(&r.mux), []);
    assert_eq!(held(&r.sup.state).len(), 1);
}

#[test]
fn a_paste_that_fails_is_typed_at_the_next_tick_with_no_hold() {
    let r = rig("sc-paste-fails");
    r.tick(1_000);
    let _ = r.send("scribe", "hi");
    r.mux.fail_type.store(true, Ordering::Relaxed);
    r.tick(2_000);
    assert_eq!(held(&r.sup.state).len(), 1);
    assert!(!steps(&r.sup.state).iter().any(|s| s == "MESSAGE_DELIVERED"));
    r.mux.fail_type.store(false, Ordering::Relaxed);
    r.tick(2_100);
    assert_eq!(typed(&r.mux).len(), 1);
}

#[test]
fn held_messages_survive_a_restart_and_an_off_fleets_seat_still_gets_them() {
    let r = rig("sc-restart");
    r.tick(1_000);
    let off = r.ask(Request::Stop(request::Stop::All));
    assert!(matches!(off, Reply::Response(Response::Stop(_))), "{off:?}");
    let _ = r.send("scribe", "hi");
    r.restarted().tick(5_000);
    assert_eq!(typed(&r.mux).len(), 1);
}

#[test]
fn a_seat_message_stays_held_while_its_seat_backs_off() {
    let r = rig("sc-backoff");
    r.tick(1_000);
    kill_all(&r.mux);
    r.tick(2_000);
    // A second quick end: the seat waits 5 s before its next revival.
    kill_all(&r.mux);
    let m = r.send("scribe", "hi");
    r.tick(3_000);
    assert!(!r.sup.tending.down().is_empty());
    assert_eq!(typed(&r.mux), []);
    assert_eq!(held(&r.sup.state).len(), 1, "held, not dropped");
    assert!(!steps(&r.sup.state).iter().any(|s| s == "MESSAGE_DROPPED"));
    assert!(waiting_on(&r.wait(&m, &[])).is_some());
    r.tick(8_000);
    let new = r.seated();
    assert_eq!(typed(&r.mux), [(new.pane, "hi".to_owned())]);
}

#[test]
fn a_seat_message_waits_for_the_revived_session() {
    let r = rig("sc-revive");
    r.tick(1_000);
    let old = r.seated().id;
    kill_all(&r.mux);
    let m = r.send("scribe", "hi");
    // The tick revives the seat, then delivers to the new session.
    r.tick(2_000);
    let new = r.seated();
    assert_ne!(new.id, old);
    assert_eq!(typed(&r.mux), [(new.pane, "hi".to_owned())]);
    assert_eq!(
        r.wait(&m, &[]),
        Reply::Response(Response::Wait(response::Wait::Delivered {
            message: m.clone(),
            session: new.id
        }))
    );
}

#[test]
fn a_gone_seat_sessions_messages_drop_at_its_revival() {
    let r = rig("sc-drop-gone");
    r.tick(1_000);
    let id = r.seated().id;
    let m = r.send(&id, "hi");
    kill_all(&r.mux);
    r.tick(3_000);
    assert_eq!(held(&r.sup.state), []);
    assert_eq!(
        typed(&r.mux),
        [],
        "a message to a session never follows its seat"
    );
    assert_eq!(
        r.wait(&m, &[]),
        refused(status::Wait::Dropped {
            message: m.clone(),
            why: Dropped::SessionEnded
        })
    );
}

#[test]
fn ending_a_seat_revives_it_fresh_and_keeps_the_seats_messages() {
    let r = rig("sc-end-seat");
    r.tick(1_000);
    for (n, now) in [(1, 2_000), (2, 3_000)] {
        let first = r.seated().id;
        let _ = r.send("scribe", "for the seat");
        let _ = r.send(&first, "for the session");
        assert_eq!(
            r.end("scribe"),
            Reply::Response(Response::End(response::End::Ended {
                session: first.clone(),
                seat: Some("scribe".into()),
            })),
            "end {n}"
        );
        let left: Vec<String> = held(&r.sup.state).into_iter().map(|m| m.text).collect();
        assert_eq!(left, ["for the seat"], "end {n}");
        r.tick(now);
        let new = r.seated();
        assert_ne!(new.id, first, "end {n}: a fresh session");
        assert!(r.sup.tending.down().is_empty(), "end {n}: no backoff");
        assert_eq!(
            typed(&r.mux).last(),
            Some(&(new.pane, "for the seat".to_owned()))
        );
    }
}
