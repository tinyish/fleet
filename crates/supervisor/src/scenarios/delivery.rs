//! What is typed in, and when: messages sent, held and delivered through
//! the paste hold, and dropped with what ends their session or seat.

use std::path::Path;
use std::sync::PoisonError;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use fleet_core::agent::{Activity, SessionState, StateName};
use fleet_core::messages::{Dropped, HOLD_MILLIS, MAX_TEXT_BYTES, Recipient, Sender};
use fleet_core::protocol::{Reply, Request, Response, Status, Waited, request, response};
use fleet_core::status;

use super::{Fakes, Rig, next_of, reached, refused, rig, rig_with, waiting_on};
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
    assert_eq!(held(&r.sup), []);
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
fn a_paste_goes_in_from_the_ticks_reading_before_any_launch() {
    // The seat's revival takes half a second, after the spawned session
    // was read idle.
    let r = rig_with(
        "sc-late",
        Fakes {
            mux: FakeMux {
                launch_delay: Duration::from_millis(500),
                ..FakeMux::default()
            },
            ..Fakes::default()
        },
    );
    let id = r.spawn();
    r.tick(1_000);
    let _ = r.send(&id, "hi");
    let seat = r.seated().id;
    for p in r
        .mux
        .panes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .iter_mut()
        .filter(|p| p.session == seat)
    {
        p.dead = true;
    }
    let launched = || {
        r.mux
            .launched
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    };
    let before = launched();
    thread::scope(|s| {
        s.spawn(|| r.tick(2_000));
        let deadline = Instant::now() + Duration::from_secs(5);
        while launched() == before && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(typed(&r.mux).len(), 1, "typed before the revival runs");
    });
    // Held from the reading that saw it idle.
    let _ = r.send(&id, "again");
    r.tick(2_000 + HOLD_MILLIS - 1);
    assert_eq!(typed(&r.mux).len(), 1, "still held");
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
    assert_eq!(held(&r.sup).len(), 1);
}

#[test]
fn a_paste_that_fails_is_typed_at_the_next_tick_with_no_hold() {
    let r = rig("sc-paste-fails");
    r.tick(1_000);
    let _ = r.send("scribe", "hi");
    r.mux.fail_type.store(true, Ordering::Relaxed);
    r.tick(2_000);
    assert_eq!(held(&r.sup).len(), 1);
    assert!(!steps(&r.sup).iter().any(|s| s == "MESSAGE_DELIVERED"));
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
fn a_seat_message_stays_held_while_its_seat_backs_off_and_its_gone_sessions_drop() {
    let r = rig("sc-backoff");
    r.tick(1_000);
    kill_all(&r.mux);
    r.tick(2_000);
    let to_session = r.send(&r.seated().id, "for the session");
    // A second quick end: the seat waits 5 s before its next revival.
    kill_all(&r.mux);
    let m = r.send("scribe", "hi");
    r.tick(3_000);
    assert!(!r.sup.tending.down().is_empty());
    assert_eq!(typed(&r.mux), []);
    let left: Vec<String> = held(&r.sup).into_iter().map(|m| m.text).collect();
    assert_eq!(left, ["hi"], "the seat's held, its gone session's dropped");
    assert_eq!(
        r.wait(&to_session, &[]),
        refused(status::Wait::Dropped {
            message: to_session.clone(),
            why: Dropped::SessionGone
        })
    );
    assert!(waiting_on(&r.wait(&m, &[])).is_some());
    // Revived at 8 s, read idle and typed into at the next tick.
    r.tick(8_000);
    r.tick(9_000);
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
    // The tick revives the seat; the next delivers to the new session.
    r.tick(2_000);
    r.tick(3_000);
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
fn a_gone_seat_sessions_messages_drop_and_never_follow_its_seat() {
    let r = rig("sc-drop-gone");
    r.tick(1_000);
    let id = r.seated().id;
    let m = r.send(&id, "hi");
    kill_all(&r.mux);
    r.tick(3_000);
    assert_eq!(held(&r.sup), []);
    assert_eq!(
        typed(&r.mux),
        [],
        "a message to a session never follows its seat"
    );
    assert_eq!(
        r.wait(&m, &[]),
        refused(status::Wait::Dropped {
            message: m.clone(),
            why: Dropped::SessionGone
        })
    );
}

#[test]
fn a_spawned_sessions_messages_drop_once_it_is_gone_and_their_wait_ends() {
    let r = rig("sc-drop-spawned-gone");
    let id = r.spawn();
    let m = r.send(&id, "hi");
    let w = r.wait(&m, &[]);
    assert!(waiting_on(&w).is_some(), "{w:?}");
    kill_all(&r.mux);
    r.tick(1_000);
    assert_eq!(held(&r.sup), []);
    let dropped = refused(status::Wait::Dropped {
        message: m.clone(),
        why: Dropped::SessionGone,
    });
    let on = Waited::Message { message: m.clone() };
    assert_eq!(r.wait_on(on, &[], next_of(&w)), dropped);
    assert_eq!(r.wait(&m, &[]), dropped);
    assert_eq!(
        r.end(&r.app, &id),
        Reply::Response(Response::End(response::End::Ended {
            session: id.clone(),
            seat: None
        }))
    );
    let drops = steps(&r.sup)
        .iter()
        .filter(|s| *s == "MESSAGE_DROPPED")
        .count();
    assert_eq!(drops, 1, "ending it later drops nothing more");
}

#[test]
fn a_message_to_a_session_dead_since_the_last_tick_is_taken_then_dropped_at_the_next() {
    let r = rig("sc-died-since");
    let id = r.spawn();
    r.tick(1_000);
    kill_all(&r.mux);
    let m = r.send(&id, "hi");
    assert_ne!(m, "", "live as last recorded, so taken");
    r.tick(2_000);
    assert_eq!(held(&r.sup), []);
    assert_eq!(typed(&r.mux), []);
    assert_eq!(
        r.wait(&m, &[]),
        refused(status::Wait::Dropped {
            message: m.clone(),
            why: Dropped::SessionGone
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
            r.end(&r.app, "scribe"),
            Reply::Response(Response::End(response::End::Ended {
                session: first.clone(),
                seat: Some("scribe".into()),
            })),
            "end {n}"
        );
        let left: Vec<String> = held(&r.sup).into_iter().map(|m| m.text).collect();
        assert_eq!(left, ["for the seat"], "end {n}");
        r.tick(now);
        r.tick(now + 500);
        let new = r.seated();
        assert_ne!(new.id, first, "end {n}: a fresh session");
        assert!(r.sup.tending.down().is_empty(), "end {n}: no backoff");
        assert_eq!(
            typed(&r.mux).last(),
            Some(&(new.pane, "for the seat".to_owned()))
        );
    }
}

/// What a person sending `text` to `to` from `from` is answered.
fn sent(r: &Rig, from: &Path, to: &str, text: &str) -> Reply {
    r.ask(Request::Message(request::Message::Send {
        directory: from.to_path_buf(),
        to: to.into(),
        text: text.into(),
        sender: Sender::Person,
    }))
}

/// Who a send's reply says the message was queued for.
fn queued(reply: &Reply) -> Option<&Recipient> {
    match reply {
        Reply::Response(Response::Message(response::Message::Queued { message, to })) => {
            assert!(message.starts_with('m') && message.len() == 11, "{message}");
            Some(to)
        }
        _ => None,
    }
}

#[test]
fn a_seat_here_is_queued_for() {
    let r = rig("msg-seat");
    let src = r.app.join("src");
    assert!(std::fs::create_dir_all(&src).is_ok());
    let reply = sent(&r, &src, "scribe", "hi");
    assert_eq!(
        queued(&reply),
        Some(&Recipient::Seat {
            fleet: r.app.clone(),
            seat: "scribe".into()
        })
    );
    let held = held(&r.sup);
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].text, "hi");
    assert_eq!(held[0].sender, Sender::Person);
}

#[test]
fn an_id_reaches_a_session_from_outside_every_fleet() {
    let r = rig("msg-id");
    r.tick(1_000);
    let id = r.seated().id;
    let reply = sent(&r, &r.root.join("wing"), &id, "hi");
    assert_eq!(queued(&reply), Some(&Recipient::Session { session: id }));
}

#[test]
fn text_that_breaks_the_rules_queues_nothing() {
    let r = rig("msg-text");
    assert_eq!(
        r.send_reply("scribe", " \n"),
        Reply::Status(status::Message::EmptyText.into())
    );
    let long = "a".repeat(MAX_TEXT_BYTES + 1);
    let reply = r.send_reply("scribe", &long);
    assert!(
        matches!(
            reply,
            Reply::Status(Status::Message(status::Message::TooLong { .. }))
        ),
        "{reply:?}"
    );
    assert_eq!(held(&r.sup), []);
}
