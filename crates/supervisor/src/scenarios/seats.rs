//! What is launched, spawned and ended: the tending loop keeping each seat
//! filled and backing it off, sessions spawned from a role, and end.

use std::path::PathBuf;
use std::sync::PoisonError;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use fleet_core::agent::{Activity, AdapterFault, SessionState, Verb};
use fleet_core::protocol::request::ConfigChoice;
use fleet_core::protocol::{Reply, Request, Response, Status, response};
use fleet_core::record::Step;
use fleet_core::seats::STEADY_MILLIS;
use fleet_core::status::{self, Takes};

use super::{CONFIG, Fakes, Rig, bare, crew, rig, rig_with, roles_only, spawned};
use crate::testing::{FakeAgents, FakeMux, code, kill_all};

/// A role of a crew: `worker`, running sonnet in the fleet's directory,
/// woken with "You take tasks.".
const WORKER: &str =
    "[roles.worker]\nadapter = \"/a/claude\"\nmodel = \"sonnet\"\nprompt = \"You take tasks.\"\n";

fn launches(r: &Rig) -> usize {
    r.agents
        .launches
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len()
}

fn reads(r: &Rig) -> usize {
    r.agents
        .reads
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len()
}

fn panes(r: &Rig) -> usize {
    r.mux
        .panes
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .len()
}

/// The scribe seat, as status shows it from the fleet.
fn scribe(r: &Rig) -> response::SeatOverview {
    let o = r.status(&r.app);
    let seat = o.fleets.into_iter().flat_map(|f| f.seats).next();
    assert!(seat.is_some(), "no seat");
    seat.unwrap_or_else(|| unreachable!())
}

/// The states recorded, in order, as (session, state).
fn states(r: &Rig) -> Vec<(String, SessionState)> {
    r.record()
        .into_iter()
        .filter_map(|step| match step {
            Step::SessionStateChanged { session, state, .. } => Some((session, state)),
            _ => None,
        })
        .collect()
}

fn launched_steps(r: &Rig) -> usize {
    r.kinds()
        .iter()
        .filter(|s| *s == "SESSION_LAUNCHED")
        .count()
}

#[test]
fn a_tick_fills_a_seat_from_its_role_and_a_second_tick_keeps_it() {
    let r = rig("seat-fill");
    r.tick(1_000);
    assert_eq!(
        *r.agents
            .launches
            .lock()
            .unwrap_or_else(PoisonError::into_inner),
        [PathBuf::from("/a/claude")]
    );
    assert_eq!(panes(&r), 1);
    r.tick(2_000);
    assert_eq!(launches(&r), 1, "a second tick keeps it");
    assert_eq!(reads(&r), 1, "every live session is read once a tick");
    let seat = scribe(&r);
    assert_eq!(seat.session.map(|s| s.state), Some(SessionState::Idle));
    assert!(!seat.older_role, "launched from its role as it is");
    let woken: Vec<(Option<String>, Option<String>)> = r
        .record()
        .into_iter()
        .filter_map(|step| match step {
            Step::SessionLaunched { seat, prompt, .. } => Some((seat, prompt)),
            _ => None,
        })
        .collect();
    assert_eq!(
        woken,
        [(Some("scribe".into()), Some("You write the notes.".into()))]
    );
}

#[test]
fn a_seat_whose_agent_died_is_recorded_gone_then_launched_again_in_the_same_tick() {
    let r = rig("seat-revive");
    r.tick(1_000);
    let old = r.seated().id;
    kill_all(&r.mux);
    r.tick(2_000);
    assert_eq!(launches(&r), 2);
    assert_eq!(panes(&r), 1, "the dead session's tmux session ended");
    let s = r.record();
    let gone = s.iter().position(|step| {
        matches!(step, Step::SessionStateChanged { session, state: SessionState::Gone, .. }
            if *session == old)
    });
    let again = s
        .iter()
        .rposition(|step| matches!(step, Step::SessionLaunched { .. }));
    assert!(gone.is_some() && gone < again, "{s:?}");
    assert_ne!(r.seated().id, old, "its gone session is no longer its own");
}

#[test]
fn a_seat_that_keeps_dying_waits_longer_between_revivals() {
    let r = rig("seat-dying");
    r.tick(1_000);
    kill_all(&r.mux);
    r.tick(2_000);
    assert_eq!(launches(&r), 2, "the first revival is immediate");
    kill_all(&r.mux);
    for now in [3_000, 4_000, 5_000, 7_999] {
        r.tick(now);
    }
    assert_eq!(launches(&r), 2, "the second waits 5 s");
    assert_eq!(
        scribe(&r).down.map(|d| d.next_try_millis),
        Some(8_000),
        "the same death is counted once"
    );
    r.tick(8_000);
    assert_eq!(launches(&r), 3);
}

#[test]
fn a_session_that_lived_ten_minutes_resets_the_backoff() {
    let r = rig("seat-steady");
    r.tick(1_000);
    kill_all(&r.mux);
    // Revived at once, with that quick end counted against the seat: ended
    // now, it would show down.
    r.tick(2_000);
    let launched = scribe(&r).session.map(|s| s.launch_time_millis);
    r.tick(launched.unwrap_or_default() + STEADY_MILLIS);
    kill_all(&r.mux);
    assert_eq!(scribe(&r).down, None, "nothing is counted against it now");
}

#[test]
fn a_restarted_supervisor_adopts_the_seats_session_and_records_nothing_unchanged() {
    let r = rig("seat-adopt");
    r.tick(1_000);
    r.restarted().tick(5_000);
    assert_eq!(panes(&r), 1);
    assert_eq!(launches(&r), 1);
    assert_eq!(states(&r).len(), 1);
}

#[test]
fn off_launches_nothing_and_ends_nothing() {
    let r = rig("seat-off");
    assert!(matches!(r.stop_all(), Reply::Response(_)));
    r.tick(1_000);
    assert_eq!(launches(&r), 0, "an off fleet's seat is not filled");
    assert!(matches!(r.start_all(), Reply::Response(_)));
    r.tick(2_000);
    assert!(matches!(r.stop_all(), Reply::Response(_)));
    kill_all(&r.mux);
    r.tick(3_000);
    assert_eq!(panes(&r), 1, "nothing launched, nothing ended");
    assert_eq!(launches(&r), 1);
    assert_eq!(
        scribe(&r).session.map(|s| s.state),
        Some(SessionState::Gone),
        "its gone session is kept"
    );
}

/// The orphans status shows from the fleet: (seat, state).
fn orphans(r: &Rig) -> Vec<(String, SessionState)> {
    r.status(&r.app)
        .fleets
        .into_iter()
        .flat_map(|f| f.orphans)
        .map(|o| (o.seat, o.session.state))
        .collect()
}

#[test]
fn a_removed_seats_session_is_left_running_and_never_revived() {
    let r = rig("seat-orphan");
    r.tick(1_000);
    assert!(std::fs::write(r.app.join("fleet.toml"), roles_only()).is_ok());
    assert!(matches!(r.start_all(), Reply::Response(_)));
    r.tick(1_500);
    assert_eq!(orphans(&r), [("scribe".into(), SessionState::Idle)]);
    kill_all(&r.mux);
    r.tick(2_000);
    assert_eq!(launches(&r), 1, "never revived");
    assert_eq!(orphans(&r), [("scribe".into(), SessionState::Gone)]);
}

#[test]
fn a_failing_launch_backs_off_shows_down_and_records_each_failure() {
    let r = rig_with(
        "seat-fail",
        Fakes {
            agents: FakeAgents {
                launch_fault: Some(AdapterFault::Exited {
                    code: Some(1),
                    said: "no".into(),
                }),
                ..FakeAgents::default()
            },
            ..Fakes::default()
        },
    );
    r.tick(1_000);
    r.tick(1_000);
    r.tick(2_000);
    assert_eq!(launches(&r), 2, "once, again at once, then not before 5 s");
    let seat = scribe(&r);
    assert_eq!(seat.session, None);
    assert!(
        seat.down
            .as_ref()
            .is_some_and(|d| d.why.is_some() && d.next_try_millis == 6_000),
        "{:?}",
        seat.down
    );
    let failed = r
        .kinds()
        .iter()
        .filter(|s| *s == "SEAT_LAUNCH_FAILED")
        .count();
    assert_eq!(failed, 2);
}

#[test]
fn a_tmux_that_cannot_list_launches_nothing_records_nothing_and_status_says_so() {
    let r = rig("seat-blind");
    r.mux.missing.store(true, Ordering::Relaxed);
    r.tick(1_000);
    assert_eq!(launches(&r), 0);
    assert!(!r.kinds().iter().any(|s| s == "SEAT_LAUNCH_FAILED"));
    let shown = r.ask(Request::Overview {
        directory: r.app.clone(),
    });
    assert!(
        matches!(
            shown,
            Reply::Status(Status::Session(
                status::Session::MultiplexerCouldNotRun { .. }
            ))
        ),
        "{shown:?}"
    );
    r.mux.missing.store(false, Ordering::Relaxed);
    let down = scribe(&r).down;
    assert!(
        down.as_ref().is_some_and(|d| d.next_try_millis == 1_000
            && matches!(
                d.why,
                Some(Status::Session(
                    status::Session::MultiplexerCouldNotRun { .. }
                ))
            )),
        "down with why: {down:?}"
    );
    r.tick(1_500);
    assert_eq!(launches(&r), 1, "tmux back: the seat is filled at once");
    let idle = states(&r);
    r.agents.read_as(Activity::Working);
    r.mux.missing.store(true, Ordering::Relaxed);
    r.tick(2_000);
    assert_eq!(states(&r), idle, "nothing is known, so nothing recorded");
    r.mux.missing.store(false, Ordering::Relaxed);
    r.tick(2_500);
    assert_eq!(states(&r).last().map(|s| s.1), Some(SessionState::Working));
    assert_eq!(launches(&r), 1);
}

#[test]
fn a_request_that_leaves_work_wakes_the_tending_loop_before_its_tick() {
    let r = rig("seat-poke");
    // The rig's create rang already: that wait returns at once.
    r.sup.tending.wait();
    let started = Instant::now();
    thread::scope(|s| {
        s.spawn(|| {
            thread::sleep(Duration::from_millis(50));
            let _ = r.start_all();
        });
        r.sup.tending.wait();
    });
    assert!(started.elapsed() < crate::tending::EVERY);
}

#[test]
fn a_state_is_recorded_once_when_it_changes() {
    let r = rig("seat-record");
    for now in [1_000, 2_000, 3_000] {
        r.tick(now);
    }
    let id = r.seated().id;
    assert_eq!(states(&r), [(id.clone(), SessionState::Idle)]);
    r.agents.read_as(Activity::Working);
    r.tick(4_000);
    assert_eq!(
        states(&r),
        [
            (id.clone(), SessionState::Idle),
            (id, SessionState::Working)
        ]
    );
}

#[test]
fn a_spawn_runs_in_the_roles_directory_and_records_its_fleet_and_role() {
    let (r, wing) = crew("spawn-role", Fakes::default(), WORKER);
    let from = code(&r.root, "app").join("src");
    assert!(std::fs::create_dir_all(&from).is_ok());
    let reply = r.spawn_from(&from, "worker", None);
    let id = spawned(&reply);
    assert_eq!(
        reply,
        Reply::Response(Response::Spawn(response::Spawn::Spawned {
            session: id.clone(),
            fleet: wing.clone(),
            role: "worker".into(),
            directory: wing.clone(),
        }))
    );
    let fleet = r.status(&wing).fleets.into_iter().next();
    let listed: Vec<(String, String)> = fleet
        .iter()
        .flat_map(|f| &f.spawned)
        .map(|s| (s.role.clone(), s.session.session.clone()))
        .collect();
    assert_eq!(listed, [("worker".into(), id)]);
    assert_eq!(fleet.map(|f| f.seats.len()), Some(0));
    let last = r.record().pop();
    assert!(
        matches!(&last, Some(Step::SessionLaunched { role: Some(role), seat: None, prompt: Some(prompt), .. })
            if role == "worker" && prompt == "You take tasks."),
        "{last:?}"
    );
}

#[test]
fn a_spawned_agent_is_told_its_session_id() {
    let r = rig("spawn-env");
    let id = r.spawn();
    let env = r
        .mux
        .launched
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .first()
        .map(|l| l.env.clone())
        .unwrap_or_default();
    assert!(id.starts_with('s'), "{id}");
    assert_eq!(env.get(fleet_core::messages::SESSION_VAR), Some(&id));
}

#[test]
fn a_spawn_may_name_one_of_the_fleets_projects_by_any_path_to_it() {
    let (r, wing) = crew("spawn-named", Fakes::default(), WORKER);
    let lib = code(&r.root, "lib");
    for named in [lib.clone(), code(&r.root, "app").join("../lib")] {
        let reply = r.spawn_from(&wing, "worker", Some(named));
        assert!(
            matches!(&reply, Reply::Response(Response::Spawn(response::Spawn::Spawned { directory, .. })) if *directory == lib),
            "{reply:?}"
        );
    }
}

#[test]
fn a_directory_that_is_no_project_root_is_refused() {
    let (r, wing) = crew("spawn-not-project", Fakes::default(), WORKER);
    let src = code(&r.root, "app").join("src");
    assert!(std::fs::create_dir_all(&src).is_ok());
    for named in [src, wing.clone()] {
        assert_eq!(
            r.spawn_from(&wing, "worker", Some(named.clone())),
            Reply::Status(Status::Session(status::Session::NotAProject {
                path: named.clone(),
                fleet: wing.clone(),
            })),
            "{}",
            named.display()
        );
    }
    assert_eq!(launched_steps(&r), 0);
}

#[test]
fn a_spawn_outside_every_fleet_or_of_no_such_role_is_refused() {
    let (r, wing) = crew("spawn-nowhere", Fakes::default(), WORKER);
    assert_eq!(
        r.spawn_from(&r.root, "worker", None),
        Reply::Status(Status::Session(status::Session::NoFleetHere {
            directory: r.root.clone()
        }))
    );
    assert_eq!(
        r.spawn_from(&wing, "critic", None),
        Reply::Status(Status::Session(status::Session::NoSuchRole {
            role: "critic".into(),
            fleet: wing.clone(),
        }))
    );
}

#[test]
fn a_fleet_with_every_project_off_still_spawns() {
    let r = rig("spawn-off");
    assert!(matches!(r.stop_all(), Reply::Response(_)));
    assert_ne!(r.spawn(), "");
}

#[test]
fn a_spawn_tmux_cannot_run_leaves_no_session_and_no_record() {
    let failing = FakeMux {
        fail_launch: true,
        ..FakeMux::default()
    };
    let missing = FakeMux::default();
    missing.missing.store(true, Ordering::Relaxed);
    let not_found = std::io::Error::from(std::io::ErrorKind::NotFound).to_string();
    for (what, mux) in [("fails", failing), ("missing", missing)] {
        let r = rig_with(
            &format!("spawn-tmux-{what}"),
            Fakes {
                mux,
                ..Fakes::default()
            },
        );
        let reply = r.spawn_from(&r.app, "w", None);
        let said = match &reply {
            Reply::Status(Status::Session(status::Session::MultiplexerFailed { .. })) => {
                what == "fails"
            }
            Reply::Status(Status::Session(status::Session::MultiplexerCouldNotRun { error })) => {
                what == "missing" && *error == not_found
            }
            _ => false,
        };
        assert!(said, "{what}: {reply:?}");
        assert_eq!(launched_steps(&r), 0, "{what}");
        r.mux.missing.store(false, Ordering::Relaxed);
        let listed = r.status(&r.app).fleets.into_iter().flat_map(|f| f.spawned);
        assert_eq!(listed.count(), 0, "{what}: no session");
    }
}

#[test]
fn a_missing_role_directory_is_refused_before_the_adapter_runs() {
    let r = bare("spawn-missing", Fakes::default());
    let config = "[roles.w]\nadapter = \"/a/claude\"\ndirectory = \"nope\"\n";
    assert!(std::fs::write(r.app.join("fleet.toml"), config).is_ok());
    assert!(matches!(r.create(&r.app, &[]), Reply::Response(_)));
    assert_eq!(
        r.spawn_from(&r.app, "w", None),
        Reply::Status(Status::File(status::File::Missing {
            path: r.app.join("nope")
        }))
    );
    assert_eq!(launches(&r), 0);
}

#[test]
fn an_adapter_that_cannot_launch_is_refused_with_its_fault() {
    let fault = AdapterFault::Exited {
        code: Some(1),
        said: "claude: not found".into(),
    };
    let r = rig_with(
        "spawn-launch-fault",
        Fakes {
            agents: FakeAgents {
                launch_fault: Some(fault.clone()),
                ..FakeAgents::default()
            },
            ..Fakes::default()
        },
    );
    assert_eq!(
        r.spawn_from(&r.app, "w", None),
        Reply::Status(Status::Session(status::Session::AdapterFailed {
            adapter: PathBuf::from("/a/claude"),
            verb: Verb::Launch,
            fault
        }))
    );
}

/// Agents that take half a second to say how to launch one, so a request
/// can land while they answer.
fn slow() -> Fakes {
    Fakes {
        agents: FakeAgents {
            launch_delay: Duration::from_millis(500),
            ..FakeAgents::default()
        },
        ..Fakes::default()
    }
}

/// Returns once the adapter was asked to launch one.
fn once_launching(r: &Rig) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while launches(r) == 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_fleet_removed_while_the_adapter_answers_spawns_nothing() {
    let r = rig_with("spawn-race", slow());
    let reply = thread::scope(|s| {
        let spawning = s.spawn(|| r.spawn_from(&r.app, "w", None));
        once_launching(&r);
        let removed = r.remove(&r.app, &[], ConfigChoice::Keep);
        assert!(matches!(removed, Reply::Response(_)), "{removed:?}");
        spawning.join().ok()
    });
    assert_eq!(
        reply,
        Some(Reply::Status(Status::Session(
            status::Session::NoFleetHere {
                directory: r.app.clone()
            }
        )))
    );
    assert_eq!(panes(&r), 0);
    assert_eq!(launched_steps(&r), 0);
}

/// A change a person makes, as the requests and files it takes.
type Change = fn(&Rig) -> Reply;

#[test]
fn a_seat_no_longer_wanted_by_the_time_it_launches_starts_nothing() {
    // Each change lands while the adapter is asked, after the tick read
    // what was wanted.
    let changes: [(&str, Change); 4] = [
        ("off", Rig::stop_all),
        ("seat-removed", |r| {
            assert!(std::fs::write(r.app.join("fleet.toml"), roles_only()).is_ok());
            r.start_all()
        }),
        ("role-edited", |r| {
            let edited = CONFIG.replace("You write the notes.", "Another prompt.");
            assert!(std::fs::write(r.app.join("fleet.toml"), edited).is_ok());
            r.start_all()
        }),
        ("fleet-removed", |r| {
            r.remove(&r.app, &[], ConfigChoice::Keep)
        }),
    ];
    for (what, change) in changes {
        let r = rig_with(&format!("seat-unwanted-{what}"), slow());
        thread::scope(|s| {
            s.spawn(|| r.tick(1_000));
            once_launching(&r);
            assert!(matches!(change(&r), Reply::Response(_)), "{what}");
        });
        assert_eq!(launches(&r), 1, "{what}: the adapter was asked");
        assert_eq!(panes(&r), 0, "{what}");
        assert_eq!(launched_steps(&r), 0, "{what}");
    }
}

#[test]
fn a_spawned_session_ends_by_its_id_and_a_second_end_changes_nothing() {
    let r = rig("end-spawned");
    let id = r.spawn();
    assert_eq!(
        r.end(&r.root, &id),
        Reply::Response(Response::End(response::End::Ended {
            session: id.clone(),
            seat: None
        }))
    );
    assert_eq!(panes(&r), 0);
    assert!(r.status(&r.app).fleets.iter().all(|f| f.spawned.is_empty()));
    let before = r.kinds();
    assert_eq!(before.last().map(String::as_str), Some("SESSION_ENDED"));
    assert_eq!(
        r.end(&r.app, &id),
        Reply::Response(Response::End(response::End::AlreadyEnded { session: id }))
    );
    assert_eq!(r.kinds(), before, "nothing recorded");
}

#[test]
fn ending_a_seat_session_nothing_revives_names_no_seat() {
    let changes: [(&str, Change); 3] = [
        ("orphaned", |r| {
            assert!(std::fs::write(r.app.join("fleet.toml"), roles_only()).is_ok());
            r.start_all()
        }),
        ("loose", |r| r.remove(&r.app, &[], ConfigChoice::Keep)),
        ("off", |r| r.stop_all()),
    ];
    for (what, change) in changes {
        let r = rig(&format!("end-unrevived-{what}"));
        r.tick(1_000);
        let id = r.seated().id;
        assert!(matches!(change(&r), Reply::Response(_)), "{what}");
        assert_eq!(
            r.end(&r.root, &id),
            Reply::Response(Response::End(response::End::Ended {
                session: id.clone(),
                seat: None,
            })),
            "{what}"
        );
    }
}

#[test]
fn ending_a_seats_old_gone_session_records_it_and_names_no_seat() {
    let r = rig("end-old-gone");
    r.tick(1_000);
    let old = r.seated().id;
    kill_all(&r.mux);
    r.tick(2_000);
    assert_ne!(r.seated().id, old, "revived");
    assert_eq!(
        r.end(&r.root, &old),
        Reply::Response(Response::End(response::End::Ended {
            session: old.clone(),
            seat: None,
        }))
    );
    let last = r.record().pop();
    assert!(
        matches!(&last, Some(Step::SessionEnded { session, .. }) if *session == old),
        "{last:?}"
    );
}

#[test]
fn a_seat_with_no_live_session_is_refused() {
    let r = rig("end-not-running");
    assert_eq!(
        r.end(&r.app, "scribe"),
        Reply::Status(Status::Session(status::Session::SeatNotRunning {
            seat: "scribe".into(),
            fleet: r.app.clone(),
        }))
    );
}

#[test]
fn a_tmux_that_fails_to_end_changes_nothing() {
    let r = rig("end-tmux");
    let id = r.spawn();
    r.mux.fail_stop.store(true, Ordering::Relaxed);
    assert!(matches!(
        r.end(&r.app, &id),
        Reply::Status(Status::Session(status::Session::MultiplexerFailed { .. }))
    ));
    let listed = r.status(&r.app).fleets.into_iter().flat_map(|f| f.spawned);
    assert_eq!(listed.count(), 1);
    let s = r.kinds();
    assert!(!s.iter().any(|s| s == "SESSION_ENDED"), "{s:?}");
}

#[test]
fn a_seat_wins_over_a_session_of_the_same_name() {
    let r = rig("names-shadow");
    let id = r.spawn();
    let config = format!("{CONFIG}\n[seats.{id}]\nrole = \"w\"\n");
    assert!(std::fs::write(r.app.join("fleet.toml"), config).is_ok());
    assert!(matches!(r.start_all(), Reply::Response(_)));
    assert_eq!(
        r.end(&r.app, &id),
        Reply::Status(Status::Session(status::Session::SeatNotRunning {
            seat: id.clone(),
            fleet: r.app.clone(),
        }))
    );
    assert_eq!(
        r.end(&r.root, &id),
        Reply::Response(Response::End(response::End::Ended {
            session: id,
            seat: None
        })),
        "outside the fleet, it names the session"
    );
}

#[test]
fn a_message_id_names_nothing_to_end_or_send_to() {
    let r = rig("names-message");
    let m = r.send("scribe", "hi");
    let unknown = |fleet: Option<PathBuf>| {
        Reply::Status(
            status::Naming::Unknown {
                name: m.clone(),
                fleet,
                takes: Takes::SeatOrSession,
            }
            .into(),
        )
    };
    assert_eq!(r.end(&r.root, &m), unknown(None));
    assert_eq!(r.send_reply(&m, "again"), unknown(Some(r.app.clone())));
}

#[test]
fn nothing_by_that_name_is_refused_with_the_fleet_searched_and_what_the_verb_takes() {
    let r = rig("names-unknown");
    for (from, here) in [(&r.app, Some(r.app.clone())), (&r.root, None)] {
        assert_eq!(
            r.end(from, "critic"),
            Reply::Status(
                status::Naming::Unknown {
                    name: "critic".into(),
                    fleet: here,
                    takes: Takes::SeatOrSession,
                }
                .into()
            )
        );
    }
}
