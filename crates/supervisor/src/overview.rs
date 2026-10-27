//! What `fleet status` shows: the fleet holding the directory it ran in,
//! with each seat's session and state, the sessions of seats its config
//! dropped, its spawned sessions, and the seats backing off; or, when no fleet holds it, every
//! fleet and the sessions of none. States come through observe.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use fleet_core::agent::SessionState;
use fleet_core::fleets;
use fleet_core::messages::Recipient;
use fleet_core::protocol::{Reply, Response, response};
use fleet_core::seats::{Roster, SeatKey};
use state::{FleetRoster, MessageRow, SessionRow, State, now_millis};

use crate::tending::{Down, Tending};
use crate::{Error, Tools, lock};

/// The overview from `directory` (made canonical when it can be): the
/// fleet holding it, or every fleet and the loose sessions.
pub(crate) fn overview(
    state: &Mutex<State>,
    tools: &Tools,
    tending: &Tending,
    directory: &Path,
) -> Result<Reply, Error> {
    let directory = std::fs::canonicalize(directory).unwrap_or_else(|_| directory.to_path_buf());
    let (fleets, rosters, rows, held) = {
        let s = lock(state);
        (s.fleets()?, s.rosters()?, s.sessions()?, s.held()?)
    };
    let here = fleets::holding(&fleets, &directory).map(|f| f.directory.clone());
    let rosters: Vec<FleetRoster> = rosters
        .into_iter()
        .filter(|f| here.as_ref().is_none_or(|h| *h == f.fleet))
        .collect();
    let registered = |row: &SessionRow| {
        row.fleet
            .as_ref()
            .is_some_and(|f| rosters.iter().any(|r| r.fleet == *f))
    };
    let shown: Vec<SessionRow> = rows
        .into_iter()
        .filter(|row| here.is_none() || registered(row))
        .collect();
    let observed = match crate::observe::observe(tools, &shown, now_millis()) {
        Ok(observed) => observed,
        Err(e) => return Ok(Reply::Status(crate::sessions::tool_status(e))),
    };
    let listed = |row: &SessionRow| response::Listed {
        session: row.id.clone(),
        adapter: row.adapter.clone(),
        directory: row.directory.clone(),
        state: observed
            .states
            .get(&row.id)
            .copied()
            .unwrap_or(SessionState::Unknown),
        launch_time_millis: row.launch_time_millis,
        waiting: held
            .iter()
            .filter(|m| matches!(&m.to, Recipient::Session { session } if *session == row.id))
            .count(),
    };
    let down = tending.down();
    let fleets = rosters
        .iter()
        .map(|f| fleet_overview(f, &shown, &down, &held, &listed))
        .collect();
    let loose = shown
        .iter()
        .filter(|row| !registered(row))
        .map(&listed)
        .collect();
    Ok(Reply::Response(Response::Overview(response::Overview {
        fleets,
        loose,
        faults: observed.faults,
    })))
}

/// One fleet's seats, with their sessions among `rows` and the messages
/// `held` for each, its orphans, and its spawned sessions.
fn fleet_overview(
    f: &FleetRoster,
    rows: &[SessionRow],
    down: &BTreeMap<SeatKey, Down>,
    held: &[MessageRow],
    listed: &impl Fn(&SessionRow) -> response::Listed,
) -> response::FleetOverview {
    let mine: Vec<&SessionRow> = rows
        .iter()
        .filter(|r| r.fleet.as_ref() == Some(&f.fleet))
        .collect();
    let seats = f
        .roster
        .seats
        .iter()
        .map(|seat| {
            let key = SeatKey {
                fleet: f.fleet.clone(),
                seat: seat.name.clone(),
            };
            let of_seat: Vec<&SessionRow> = mine
                .iter()
                .copied()
                .filter(|r| r.seat.as_deref() == Some(seat.name.as_str()))
                .collect();
            let waiting = held
                .iter()
                .filter(|m| {
                    matches!(&m.to, Recipient::Seat { fleet, seat: s }
                        if *fleet == f.fleet && *s == seat.name)
                })
                .count();
            response::SeatOverview {
                waiting,
                ..seat_overview(&f.roster, seat, &of_seat, down.get(&key), listed)
            }
        })
        .collect();
    let orphans = mine
        .iter()
        .filter_map(|r| {
            let seat = r.seat.as_ref()?;
            f.roster
                .seats
                .iter()
                .all(|s| s.name != *seat)
                .then(|| response::Orphan {
                    seat: seat.clone(),
                    session: listed(r),
                })
        })
        .collect();
    let spawned = mine
        .iter()
        .filter(|r| r.seat.is_none())
        .filter_map(|r| {
            Some(response::Spawned {
                role: r.role.clone()?,
                session: listed(r),
            })
        })
        .collect();
    response::FleetOverview {
        directory: f.fleet.clone(),
        on: f.on,
        seats,
        orphans,
        spawned,
    }
}

/// One seat: its session (the live one, else the newest), whether that was
/// launched from an older role, and its backoff while nothing of it lives.
fn seat_overview(
    roster: &Roster,
    seat: &fleet_core::seats::Seat,
    rows: &[&SessionRow],
    down: Option<&Down>,
    listed: &impl Fn(&SessionRow) -> response::Listed,
) -> response::SeatOverview {
    let sessions: Vec<(response::Listed, &SessionRow)> =
        rows.iter().map(|r| (listed(r), *r)).collect();
    let chosen = sessions
        .iter()
        .find(|(l, _)| l.state != SessionState::Gone)
        .or_else(|| sessions.iter().max_by_key(|(l, _)| l.launch_time_millis));
    let live = chosen.is_some_and(|(l, _)| l.state != SessionState::Gone);
    let digest = roster.role(&seat.role).map(fleet_core::seats::Role::digest);
    response::SeatOverview {
        name: seat.name.clone(),
        role: seat.role.clone(),
        older_role: chosen.is_some_and(|(_, r)| r.role_digest != digest),
        session: chosen.map(|(l, _)| l.clone()),
        down: down.filter(|_| !live).map(|d| response::Down {
            why: d.why.clone(),
            next_try_millis: d.backoff.next_try_millis,
        }),
        waiting: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Arc, PoisonError};

    use fleet_core::agent::{AdapterFault, SessionState};
    use fleet_core::protocol::Response;
    use fleet_core::seats::Roster;

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

    fn answer(
        state: &Mutex<State>,
        t: &Tools,
        tending: &Tending,
        dir: &Path,
    ) -> response::Overview {
        match overview(state, t, tending, dir) {
            Ok(Reply::Response(Response::Overview(o))) => o,
            other => {
                assert!(matches!(other, Ok(Reply::Response(_))), "{other:?}");
                response::Overview::default()
            }
        }
    }

    /// A seated fleet, filled by one tick; its tools and its tending.
    fn filled(name: &str) -> (PathBuf, Mutex<State>, Arc<FakeMux>, Tools, Tending) {
        let (root, state, _) = with_seat(name);
        let mux = Arc::new(FakeMux::default());
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        let tending = Tending::new();
        tending::tick(&state, &t, &store(&root), &tending, 1_000);
        (root, state, mux, t, tending)
    }

    #[test]
    fn waiting_messages_are_counted_for_their_seat_and_their_session() {
        let (root, state, _mux, t, tending) = filled("ov-waiting");
        let app = crate::testing::code(&root, "app");
        let id = lock(&state).sessions().unwrap_or_default()[0].id.clone();
        for (to, text) in [("scribe", "one"), ("scribe", "two"), (id.as_str(), "three")] {
            let ask = crate::messages::Send {
                directory: app.clone(),
                to: to.into(),
                text: text.into(),
                sender: fleet_core::messages::Sender::Person,
            };
            assert!(crate::messages::send(&state, &t, &ask).is_ok());
        }
        let o = answer(&state, &t, &tending, &app);
        let seat = &o.fleets[0].seats[0];
        assert_eq!(seat.waiting, 2);
        assert_eq!(seat.session.as_ref().map(|s| s.waiting), Some(1));
    }

    #[test]
    fn spawned_sessions_list_under_their_fleet_in_launch_order() {
        let (root, state, mux, t, tending) = filled("ov-spawned");
        let app = crate::testing::code(&root, "app");
        crate::testing::put(
            &state,
            &*t.mux,
            &crate::testing::spawned_row("s0000000002", &app, 20),
        );
        crate::testing::put(
            &state,
            &*t.mux,
            &crate::testing::spawned_row("s0000000001", &app, 10),
        );
        // The later one's agent ends.
        for p in mux
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
        {
            if p.session == "s0000000002" {
                p.dead = true;
            }
        }
        let o = answer(&state, &t, &tending, &app);
        let spawned: Vec<(String, String, SessionState)> = o.fleets[0]
            .spawned
            .iter()
            .map(|s| (s.role.clone(), s.session.session.clone(), s.session.state))
            .collect();
        assert_eq!(
            spawned,
            [
                ("w".into(), "s0000000001".into(), SessionState::Idle),
                ("w".into(), "s0000000002".into(), SessionState::Gone),
            ]
        );
        assert_eq!(o.fleets[0].seats.len(), 1, "a spawned session is no seat's");
        assert_eq!(o.fleets[0].orphans, []);
    }

    #[test]
    fn a_removed_fleets_spawned_session_is_loose() {
        let (root, state, _mux, t, tending) = filled("ov-spawned-loose");
        crate::testing::put(
            &state,
            &*t.mux,
            &crate::testing::spawned_row("s0000000003", Path::new("/gone"), 10),
        );
        let o = answer(&state, &t, &tending, &root.join("wing"));
        assert_eq!(
            o.loose
                .iter()
                .map(|l| l.session.clone())
                .collect::<Vec<_>>(),
            ["s0000000003"]
        );
        assert!(o.fleets.iter().all(|f| f.spawned.is_empty()));
    }

    /// A session of a fleet since removed, as `fleet status` lists loose.
    fn loose_session(state: &Mutex<State>, t: &Tools) -> String {
        let id = "s00000000aa";
        crate::testing::put(
            state,
            &*t.mux,
            &crate::testing::spawned_row(id, Path::new("/gone"), 5),
        );
        id.to_owned()
    }

    #[test]
    fn here_is_one_fleet_with_its_seats_states() {
        let (root, state, _, t, tending) = filled("ov-here");
        let _ = loose_session(&state, &t);
        let o = answer(&state, &t, &tending, &root.join("code/app/src"));
        assert_eq!(o.fleets.len(), 1);
        let f = &o.fleets[0];
        assert_eq!(f.directory, root.join("code/app"));
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
    }

    #[test]
    fn elsewhere_is_every_fleet_and_the_loose_sessions() {
        let (root, state, _, t, tending) = filled("ov-all");
        let loose = loose_session(&state, &t);
        let o = answer(&state, &t, &tending, &root.join("wing"));
        assert_eq!(o.fleets.len(), 1);
        assert_eq!(o.fleets[0].seats.len(), 1);
        assert_eq!(
            o.loose
                .iter()
                .map(|l| l.session.clone())
                .collect::<Vec<_>>(),
            [loose]
        );
    }

    #[test]
    fn a_seat_removed_from_its_roster_is_an_orphan() {
        let (root, state, _, t, tending) = filled("ov-orphan");
        let app = root.join("code/app");
        assert!(
            lock(&state)
                .write(|w| w.set_roster(&app, &Roster::default()))
                .is_ok()
        );
        let o = answer(&state, &t, &tending, &app);
        assert_eq!(o.fleets[0].seats, []);
        assert_eq!(
            o.fleets[0]
                .orphans
                .iter()
                .map(|o| o.seat.as_str())
                .collect::<Vec<_>>(),
            ["scribe"]
        );
        assert_eq!(o.loose, []);
    }

    #[test]
    fn an_edited_role_marks_its_session_older() {
        let (root, state, _, t, tending) = filled("ov-older");
        let app = root.join("code/app");
        let mut roster = lock(&state).roster(&app).unwrap_or_default();
        roster.roles[0].prompt = Some("You write the release notes.".into());
        assert!(lock(&state).write(|w| w.set_roster(&app, &roster)).is_ok());
        let o = answer(&state, &t, &tending, &app);
        assert!(o.fleets[0].seats[0].older_role);
    }

    #[test]
    fn a_seat_backing_off_shows_down_with_why() {
        let (root, state, _) = with_seat("ov-down");
        let agents = Arc::new(FakeAgents {
            launch_fault: Some(AdapterFault::Exited {
                code: Some(1),
                said: "no".into(),
            }),
            ..FakeAgents::default()
        });
        let t = tools(Arc::new(FakeMux::default()), agents);
        let tending = Tending::new();
        tending::tick(&state, &t, &store(&root), &tending, 1_000);
        tending::tick(&state, &t, &store(&root), &tending, 1_000);
        let o = answer(&state, &t, &tending, &root.join("code/app"));
        let seat = &o.fleets[0].seats[0];
        assert_eq!(seat.session, None);
        assert!(
            seat.down
                .as_ref()
                .is_some_and(|d| d.why.is_some() && d.next_try_millis == 6_000),
            "{:?}",
            seat.down
        );
    }

    #[test]
    fn a_seat_whose_session_lives_is_not_down() {
        let (root, state, mux, t, tending) = filled("ov-up");
        for p in mux
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
        {
            p.dead = true;
        }
        // Revived at once after one quick end: up again, its backoff kept.
        tending::tick(&state, &t, &store(&root), &tending, 2_000);
        assert!(!tending.down().is_empty());
        let o = answer(&state, &t, &tending, &root.join("code/app"));
        let seat = &o.fleets[0].seats[0];
        assert_eq!(
            seat.session.as_ref().map(|s| s.state),
            Some(SessionState::Idle)
        );
        assert_eq!(seat.down, None);
    }

    #[test]
    fn a_removed_fleets_session_is_loose() {
        let (root, state, _, t, tending) = filled("ov-removed");
        let app = root.join("code/app");
        assert!(lock(&state).write(|w| w.remove_fleet(&app)).is_ok());
        let o = answer(&state, &t, &tending, &app);
        assert_eq!(o.fleets, []);
        assert_eq!(o.loose.len(), 1);
    }

    #[test]
    fn here_asks_only_about_its_own_sessions() {
        let (root, state, _) = with_seat("ov-scope");
        let agents = Arc::new(FakeAgents::default());
        let t = tools(Arc::new(FakeMux::default()), agents.clone());
        let tending = Tending::new();
        tending::tick(&state, &t, &store(&root), &tending, 1_000);
        let _ = loose_session(&state, &t);
        let _ = answer(&state, &t, &tending, &root.join("code/app"));
        let asked: Vec<usize> = agents
            .reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(_, refs)| refs.len())
            .collect();
        assert_eq!(asked, [1], "the seat's session, not the loose one");
    }
}
