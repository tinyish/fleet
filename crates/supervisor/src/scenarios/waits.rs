//! What wait and events answer: at once from what is recorded, or once a
//! tick, a request or a change records what decides them.

use std::thread;
use std::time::{Duration, Instant};

use fleet_core::agent::{Activity, SessionState, StateName};
use fleet_core::messages::Dropped;
use fleet_core::protocol::request::ConfigChoice;
use fleet_core::protocol::{Reply, Request, Response, Waited, request, response};
use fleet_core::status;

use super::{Rig, next_of, reached, refused, rig, roles_only};
use crate::testing::{code, held, kill_all};

#[test]
fn ending_a_session_while_waited_on_refuses_the_wait_and_drops_its_messages() {
    let r = rig("sc-end");
    let id = r.spawn();
    r.tick(1_000);
    let w = r.wait(&id, &[StateName::Working]);
    let m = r.send(&id, "hi");
    assert_eq!(
        r.end(&r.app, &id),
        Reply::Response(Response::End(response::End::Ended {
            session: id.clone(),
            seat: None
        }))
    );
    let ended = refused(status::Wait::Ended {
        session: id.clone(),
    });
    let on = Waited::Session {
        session: id.clone(),
    };
    assert_eq!(r.wait_on(on, &[StateName::Working], next_of(&w)), ended);
    assert_eq!(
        r.wait(&id, &[StateName::Idle]),
        ended,
        "idle before it ended is not the answer"
    );
    assert_eq!(
        r.wait(&m, &[]),
        refused(status::Wait::Dropped {
            message: m.clone(),
            why: Dropped::SessionEnded
        })
    );
    assert_eq!(r.dropped_after(&m), Some(r.in_fleet("SESSION_ENDED")));
    assert_eq!(
        r.send_reply(&id, "again"),
        Reply::Status(status::Message::SessionEnded { session: id }.into())
    );
    assert_eq!(held(&r.sup), []);
}

#[test]
fn a_seat_taken_out_while_waited_on_refuses_the_wait_and_drops_its_messages() {
    let r = rig("sc-seat-out");
    r.tick(1_000);
    let m = r.send("scribe", "hi");
    let w = r.wait("scribe", &[StateName::Working]);
    assert!(std::fs::write(r.app.join("fleet.toml"), roles_only()).is_ok());
    let started = r.ask(Request::Start(request::Start::All));
    assert!(
        matches!(started, Reply::Response(Response::Start(_))),
        "{started:?}"
    );
    assert_eq!(
        r.wait_on(r.scribe(), &[StateName::Working], next_of(&w)),
        refused(status::Wait::SeatRemoved {
            fleet: r.app.clone(),
            seat: "scribe".into()
        })
    );
    assert_eq!(
        r.wait(&m, &[]),
        refused(status::Wait::Dropped {
            message: m.clone(),
            why: Dropped::SeatRemoved
        })
    );
    assert_eq!(r.dropped_after(&m), Some(r.in_fleet("SEAT_REMOVED")));
    assert_eq!(held(&r.sup), []);
}

/// A request, or a change and a request, the rig is put through.
type Change = fn(&Rig) -> Reply;

#[test]
fn a_fleet_removed_or_forgotten_refuses_a_seat_wait_and_drops_its_messages() {
    let changes: [(&str, &str, Change); 2] = [
        ("removed", "FLEET_REMOVED", |r| {
            r.ask(Request::Remove(request::Remove::Fleet {
                directory: r.app.clone(),
                projects: vec![],
                config: ConfigChoice::Keep,
            }))
        }),
        ("forgotten", "FLEET_FORGOTTEN", |r| {
            let _ = std::fs::remove_dir_all(&r.app);
            r.ask(Request::Start(request::Start::All))
        }),
    ];
    for (what, kind, change) in changes {
        let r = rig(&format!("sc-fleet-{what}"));
        r.tick(1_000);
        let m = r.send("scribe", "hi");
        let w = r.wait("scribe", &[StateName::Working]);
        assert!(matches!(change(&r), Reply::Response(_)), "{what}");
        assert_eq!(
            r.wait_on(r.scribe(), &[StateName::Working], next_of(&w)),
            refused(status::Wait::FleetRemoved {
                fleet: r.app.clone()
            }),
            "{what}"
        );
        assert_eq!(
            r.wait(&m, &[]),
            refused(status::Wait::Dropped {
                message: m.clone(),
                why: Dropped::FleetRemoved
            }),
            "{what}"
        );
        assert_eq!(r.dropped_after(&m), Some(r.in_fleet(kind)), "{what}");
        assert_eq!(held(&r.sup), [], "{what}");
    }
}

#[test]
fn a_wait_on_a_seat_or_session_needs_a_state() {
    let r = rig("sc-no-states");
    assert_eq!(
        r.wait("scribe", &[]),
        Reply::Status(
            status::Wait::NoStates {
                name: "scribe".into()
            }
            .into()
        )
    );
}

#[test]
fn a_wait_on_a_seat_answers_at_once_when_already_there_else_says_where_to_ask_from() {
    let r = rig("sc-seat-now");
    r.tick(1_000);
    let s = r.seated().id;
    assert_eq!(
        r.wait("scribe", &[StateName::Idle]),
        reached(r.scribe(), &s, SessionState::Idle)
    );
    r.agents.read_as(Activity::Working);
    r.tick(2_000);
    let next = r.sup.lock().next_id().unwrap_or_default();
    let w = r.wait("scribe", &[StateName::Idle]);
    assert_eq!(
        w,
        Reply::Response(Response::Wait(response::Wait::Waiting {
            on: r.scribe(),
            next,
            session: Some(s.clone()),
            state: Some(SessionState::Working),
        }))
    );
    // Idle comes and goes before the next poll: it still counts.
    r.agents.read_as(Activity::Idle);
    r.tick(3_000);
    r.agents.read_as(Activity::Working);
    r.tick(4_000);
    assert_eq!(
        r.wait_on(r.scribe(), &[StateName::Idle], next_of(&w)),
        reached(r.scribe(), &s, SessionState::Idle)
    );
}

#[test]
fn a_wait_on_a_seat_follows_it_across_a_revival() {
    let r = rig("sc-seat-revival");
    r.tick(1_000);
    let old = r.seated().id;
    let w = r.wait("scribe", &[StateName::Working]);
    kill_all(&r.mux);
    r.tick(2_000);
    let new = r.seated().id;
    assert_ne!(new, old);
    r.agents.read_as(Activity::Working);
    r.tick(3_000);
    assert_eq!(
        r.wait_on(r.scribe(), &[StateName::Working], next_of(&w)),
        reached(r.scribe(), &new, SessionState::Working)
    );
}

#[test]
fn a_held_wait_wakes_when_a_tick_records_its_state() {
    let r = rig("sc-wake");
    r.tick(1_000);
    let started = Instant::now();
    let reply = thread::scope(|s| {
        s.spawn(|| {
            thread::sleep(Duration::from_millis(300));
            r.agents.read_as(Activity::Working);
            r.tick(2_000);
        });
        r.ask(Request::Wait(request::Wait::For {
            directory: r.app.clone(),
            name: "scribe".into(),
            states: vec![StateName::Working],
            hold_millis: 20_000,
        }))
    });
    assert!(
        matches!(
            reply,
            Reply::Response(Response::Wait(response::Wait::Reached { .. }))
        ),
        "{reply:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn a_spawned_session_gone_is_never_revived_and_refuses_a_wait_and_a_message() {
    let r = rig("sc-spawned-gone");
    let id = r.spawn();
    r.tick(60_000);
    kill_all(&r.mux);
    r.tick(62_000);
    r.restarted().tick(63_000);
    let spawned: Vec<(String, Option<SessionState>)> = r
        .sup
        .lock()
        .sessions()
        .unwrap_or_default()
        .into_iter()
        .filter(|row| row.seat.is_none())
        .map(|row| (row.id, row.state))
        .collect();
    assert_eq!(spawned, [(id.clone(), Some(SessionState::Gone))]);
    assert_eq!(
        r.wait(&id, &[StateName::Idle]),
        refused(status::Wait::Gone {
            session: id.clone()
        })
    );
    assert_eq!(
        r.send_reply(&id, "hi"),
        Reply::Status(status::Message::SessionEnded { session: id }.into())
    );
}

/// The kinds of the steps an events reply carries; `["?"]` for any other
/// reply.
fn kinds(reply: &Reply) -> Vec<String> {
    match reply {
        Reply::Response(Response::Events(response::Events::Steps { events, .. })) => events
            .iter()
            .map(|e| e.step.to_row().map(|(kind, _)| kind).unwrap_or_default())
            .collect(),
        _ => vec!["?".into()],
    }
}

fn events(r: &Rig, directory: &std::path::Path, from: Option<i64>, hold_millis: u64) -> Reply {
    r.ask(Request::Events(request::Events::Follow {
        directory: directory.to_path_buf(),
        from,
        hold_millis,
    }))
}

#[test]
fn events_in_a_fleet_are_its_steps_with_the_supervisors_own() {
    let r = rig("sc-events");
    let api = code(&r.root, "api");
    assert!(matches!(r.create(&api, &[]), Reply::Response(_)));
    let now = events(&r, &r.app, None, 0);
    assert_eq!(kinds(&now), Vec::<String>::new(), "nothing new: empty");
    let from = match now {
        Reply::Response(Response::Events(response::Events::Steps { next, .. })) => next,
        _ => -1,
    };
    let _ = r.ask(Request::Stop(request::Stop::Project { project: api }));
    let _ = r.ask(Request::Stop(request::Stop::All));
    assert_eq!(
        kinds(&events(&r, &r.app, Some(from), 0)),
        ["EVERY_FLEET_OFF"]
    );
    assert_eq!(
        kinds(&events(&r, &r.root, Some(from), 0)),
        ["PROJECT_OFF", "EVERY_FLEET_OFF"]
    );
}

#[test]
fn events_from_now_hold_until_a_step_commits() {
    let r = rig("sc-events-hold");
    let started = Instant::now();
    let reply = thread::scope(|s| {
        s.spawn(|| {
            thread::sleep(Duration::from_millis(300));
            let _ = r.ask(Request::Stop(request::Stop::All));
        });
        events(&r, &r.app, None, 20_000)
    });
    assert_eq!(kinds(&reply), ["EVERY_FLEET_OFF"]);
    assert!(started.elapsed() < Duration::from_secs(5));
}
