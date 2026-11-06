//! What status shows: the fleet here with its seats, orphans and spawned
//! sessions, or every fleet and the loose sessions; each session's state
//! from tmux and one read of its adapter, and the adapters that failed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;
use std::thread;
use std::time::Duration;

use fleet_core::agent::{Activity, AdapterFault, Reading, SessionState};
use fleet_core::messages::Sender;
use fleet_core::protocol::request::ConfigChoice;
use fleet_core::protocol::{Reply, Request, request, response};
use fleet_core::tools::Pane;

use super::{CONFIG, Fakes, Rig, crew, rig, rig_with, roles_only, spawned};
use crate::testing::{FakeAgents, code, kill_all};

/// The spawned sessions status lists from `from`: (role, session, state).
fn spawned_of(r: &Rig, from: &Path) -> Vec<(String, String, SessionState)> {
    r.status(from)
        .fleets
        .into_iter()
        .flat_map(|f| f.spawned)
        .map(|s| (s.role, s.session.session, s.session.state))
        .collect()
}

/// What each adapter was asked to read, since the last call: its sessions'
/// pids, by adapter, in order.
fn asked(r: &Rig) -> Vec<(PathBuf, Vec<u32>)> {
    let mut reads = r
        .agents
        .reads
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    reads
        .drain(..)
        .map(|(adapter, refs)| (adapter, refs.iter().map(|s| s.pid).collect()))
        .collect()
}

/// tmux's panes, to change as tmux or the person would.
fn panes(r: &Rig) -> std::sync::MutexGuard<'_, Vec<Pane>> {
    r.mux.panes.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn spawned_sessions_list_under_their_fleet_in_launch_order_with_their_readings() {
    let r = rig("status-spawned");
    let first = r.spawn();
    thread::sleep(Duration::from_millis(5));
    let second = r.spawn();
    assert!(first.starts_with('s') && first.len() == 11, "{first}");
    for p in panes(&r).iter_mut().filter(|p| p.session == second) {
        p.dead = true;
    }
    let o = r.status(&r.app);
    assert_eq!(o.faults, []);
    let f = &o.fleets[0];
    let listed: Vec<(&str, &str, PathBuf, &Path, SessionState)> = f
        .spawned
        .iter()
        .map(|s| {
            let l = &s.session;
            (
                s.role.as_str(),
                l.session.as_str(),
                l.adapter.clone(),
                l.directory.as_path(),
                l.state,
            )
        })
        .collect();
    let claude = PathBuf::from("/a/claude");
    assert_eq!(
        listed,
        [
            (
                "w",
                first.as_str(),
                claude.clone(),
                r.app.as_path(),
                SessionState::Idle
            ),
            (
                "w",
                second.as_str(),
                claude,
                r.app.as_path(),
                SessionState::Gone
            ),
        ]
    );
    assert_eq!(f.seats.len(), 1, "a spawned session is no seat's");
    assert_eq!(f.orphans, []);
}

#[test]
fn a_session_whose_pane_ended_is_gone_without_asking_the_adapter() {
    let r = rig("status-gone");
    let id = r.spawn();
    kill_all(&r.mux);
    let _ = asked(&r);
    let o = r.status(&r.app);
    assert_eq!(
        spawned_of(&r, &r.app),
        [("w".into(), id, SessionState::Gone)]
    );
    assert_eq!(o.faults, []);
    assert_eq!(asked(&r), []);
}

#[test]
fn an_adapter_that_fails_to_read_makes_its_sessions_unknown_and_says_why() {
    let fault = AdapterFault::TimedOut {
        after_millis: 20_000,
    };
    let r = rig_with(
        "status-fault",
        Fakes {
            agents: FakeAgents {
                read_fault: Some(fault.clone()),
                ..FakeAgents::default()
            },
            ..Fakes::default()
        },
    );
    let id = r.spawn();
    let o = r.status(&r.app);
    assert_eq!(
        o.faults,
        [response::Fault {
            adapter: PathBuf::from("/a/claude"),
            fault
        }]
    );
    assert_eq!(
        spawned_of(&r, &r.app),
        [("w".into(), id, SessionState::Unknown)]
    );
}

/// Two roles, `one` and `two`, each of its own adapter.
const TWO: &str = "[roles.one]\nadapter = \"/a/one\"\n\n[roles.two]\nadapter = \"/a/two\"\n";

#[test]
fn each_adapter_is_read_once_for_its_live_sessions_only() {
    let (r, wing) = crew("status-two", Fakes::default(), TWO);
    let ids: Vec<String> = ["one", "two", "one", "one"]
        .into_iter()
        .map(|role| spawned(&r.spawn_from(&wing, role, None)))
        .collect();
    for p in panes(&r).iter_mut().filter(|p| p.session == ids[3]) {
        p.dead = true;
    }
    let _ = asked(&r);
    let states: Vec<SessionState> = spawned_of(&r, &wing).into_iter().map(|s| s.2).collect();
    let mut reads: Vec<(PathBuf, usize)> = asked(&r)
        .into_iter()
        .map(|(adapter, pids)| (adapter, pids.len()))
        .collect();
    reads.sort();
    assert_eq!(
        reads,
        [(PathBuf::from("/a/one"), 2), (PathBuf::from("/a/two"), 1)]
    );
    assert_eq!(
        states.iter().filter(|s| **s == SessionState::Gone).count(),
        1
    );
}

#[test]
fn a_split_off_pane_is_never_the_agents() {
    let r = rig("status-split");
    let id = r.spawn();
    let agent = panes(&r)[0].clone();
    // The person splits the agent's window: tmux lists the new pane last.
    panes(&r).push(Pane {
        session: id.clone(),
        id: "%9".into(),
        pid: 900,
        dead: false,
    });
    r.agents.read_as(Activity::Working);
    let _ = asked(&r);
    assert_eq!(
        spawned_of(&r, &r.app),
        [("w".into(), id.clone(), SessionState::Working)]
    );
    assert_eq!(asked(&r), [(PathBuf::from("/a/claude"), vec![agent.pid])]);
    // The agent's pane ends, then is gone; the split lives on.
    for p in panes(&r).iter_mut().filter(|p| p.id == agent.id) {
        p.dead = true;
    }
    assert_eq!(
        spawned_of(&r, &r.app),
        [("w".into(), id.clone(), SessionState::Gone)]
    );
    panes(&r).retain(|p| p.id != agent.id);
    assert_eq!(
        spawned_of(&r, &r.app),
        [("w".into(), id, SessionState::Gone)]
    );
    assert_eq!(asked(&r), [], "a gone session is not asked about");
}

#[test]
fn a_pane_id_a_restarted_tmux_gave_another_session_is_not_this_ones() {
    let r = rig("status-renumbered");
    let id = r.spawn();
    // tmux numbers panes from the start again after its server restarts:
    // the session's pane id is another session's now.
    for p in panes(&r).iter_mut() {
        p.session = "other".into();
    }
    let _ = asked(&r);
    assert_eq!(
        spawned_of(&r, &r.app),
        [("w".into(), id, SessionState::Gone)]
    );
    assert_eq!(asked(&r), [], "a gone session is not asked about");
}

#[test]
fn a_reading_about_a_session_not_asked_about_is_dropped() {
    let roles =
        "[roles.claude]\nadapter = \"/a/claude\"\n\n[roles.codex]\nadapter = \"/a/codex\"\n";
    let (r, wing) = crew("status-unasked", Fakes::default(), roles);
    let one = spawned(&r.spawn_from(&wing, "claude", None));
    let two = spawned(&r.spawn_from(&wing, "codex", None));
    let working = |session: &str| Reading {
        session: session.into(),
        activity: Activity::Working,
        reason: None,
    };
    // claude answers for its session and, wrongly, for codex's and for one
    // that is nobody's; codex answers nothing.
    *r.agents
        .readings
        .lock()
        .unwrap_or_else(PoisonError::into_inner) = Some(BTreeMap::from([(
        PathBuf::from("/a/claude"),
        vec![working(&one), working(&two), working("s9")],
    )]));
    let mut shown = spawned_of(&r, &wing);
    shown.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        shown,
        [
            ("claude".into(), one, SessionState::Working),
            ("codex".into(), two, SessionState::Unknown),
        ],
        "claude's reading of codex's session does not count"
    );
}

#[test]
fn waiting_messages_are_counted_for_their_seat_and_their_session() {
    let r = rig("status-waiting");
    r.tick(1_000);
    let id = r.seated().id;
    for (to, text) in [("scribe", "one"), ("scribe", "two"), (id.as_str(), "three")] {
        let sent = r.ask(Request::Message(request::Message::Send {
            directory: r.app.clone(),
            to: to.into(),
            text: text.into(),
            sender: Sender::Person,
        }));
        assert!(matches!(sent, Reply::Response(_)), "{sent:?}");
    }
    let seat = &r.status(&r.app).fleets[0].seats[0];
    assert_eq!(seat.waiting, 2);
    assert_eq!(seat.session.as_ref().map(|s| s.waiting), Some(1));
}

/// A session of fleet `code/api`, spawned there, then the fleet removed: a
/// session status lists loose.
fn loose_session(r: &Rig) -> String {
    let api = code(&r.root, "api");
    assert!(std::fs::write(api.join("fleet.toml"), CONFIG).is_ok());
    assert!(matches!(r.create(&api, &[]), Reply::Response(_)));
    let id = spawned(&r.spawn_from(&api, "w", None));
    let removed = r.remove(&api, &[], ConfigChoice::Keep);
    assert!(matches!(removed, Reply::Response(_)), "{removed:?}");
    id
}

#[test]
fn here_is_one_fleet_with_its_seats_states_and_asks_only_about_its_own() {
    let r = rig("status-here");
    let _ = loose_session(&r);
    r.tick(1_000);
    let src = r.app.join("src");
    assert!(std::fs::create_dir_all(&src).is_ok());
    for from in [src.clone(), src.join("..")] {
        let _ = asked(&r);
        let o = r.status(&from);
        assert_eq!(o.fleets.len(), 1, "{}", from.display());
        let f = &o.fleets[0];
        assert_eq!(f.directory, r.app);
        assert!(f.on);
        assert_eq!(f.seats.len(), 1);
        let seat = &f.seats[0];
        assert_eq!((seat.name.as_str(), seat.role.as_str()), ("scribe", "w"));
        assert_eq!(
            seat.session.as_ref().map(|s| s.state),
            Some(SessionState::Idle)
        );
        assert!(!seat.older_role);
        assert_eq!(seat.down, None);
        assert_eq!(f.orphans, []);
        assert_eq!(o.loose, [], "here shows no loose sessions");
        let pids: Vec<usize> = asked(&r).into_iter().map(|a| a.1.len()).collect();
        assert_eq!(pids, [1], "the seat's session, not the loose one");
    }
}

#[test]
fn elsewhere_is_every_fleet_and_the_loose_sessions() {
    let r = rig("status-elsewhere");
    let loose = loose_session(&r);
    r.tick(1_000);
    let ids = |l: &[response::Listed]| l.iter().map(|l| l.session.clone()).collect::<Vec<_>>();
    let o = r.status(&r.root.join("wing"));
    assert_eq!(o.fleets.len(), 1);
    assert_eq!(o.fleets[0].seats.len(), 1);
    assert!(o.fleets.iter().all(|f| f.spawned.is_empty()));
    assert_eq!(ids(&o.loose), std::slice::from_ref(&loose));
    // The fleet removed too: its seat's session is loose as well.
    let seat = r.seated().id;
    assert!(matches!(
        r.remove(&r.app, &[], ConfigChoice::Keep),
        Reply::Response(_)
    ));
    let o = r.status(&r.app);
    assert_eq!(o.fleets, []);
    let mut all = ids(&o.loose);
    all.sort();
    let mut want = vec![loose, seat];
    want.sort();
    assert_eq!(all, want);
}

#[test]
fn a_seat_removed_from_its_config_is_an_orphan() {
    let r = rig("status-orphan");
    r.tick(1_000);
    assert!(std::fs::write(r.app.join("fleet.toml"), roles_only()).is_ok());
    assert!(matches!(r.start_all(), Reply::Response(_)));
    let o = r.status(&r.app);
    assert_eq!(o.fleets[0].seats, []);
    let orphans: Vec<&str> = o.fleets[0]
        .orphans
        .iter()
        .map(|o| o.seat.as_str())
        .collect();
    assert_eq!(orphans, ["scribe"]);
    assert_eq!(o.loose, []);
}

#[test]
fn an_edited_role_marks_its_session_older() {
    let r = rig("status-older");
    r.tick(1_000);
    let edited = CONFIG.replace("You write the notes.", "You write the release notes.");
    assert!(std::fs::write(r.app.join("fleet.toml"), edited).is_ok());
    assert!(matches!(r.start_all(), Reply::Response(_)));
    assert!(r.status(&r.app).fleets[0].seats[0].older_role);
}

#[test]
fn a_seat_shows_down_only_while_none_of_its_sessions_lives() {
    let r = rig("status-up");
    r.tick(1_000);
    kill_all(&r.mux);
    // Revived at once after one quick end: up again, its backoff kept.
    r.tick(2_000);
    let seat = &r.status(&r.app).fleets[0].seats[0];
    assert_eq!(
        seat.session.as_ref().map(|s| s.state),
        Some(SessionState::Idle)
    );
    assert_eq!(seat.down, None);
    kill_all(&r.mux);
    let seat = &r.status(&r.app).fleets[0].seats[0];
    assert_eq!(seat.down.as_ref().map(|d| d.next_try_millis), Some(2_000));
}
