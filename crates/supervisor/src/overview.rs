//! What `fleet status` shows: the fleet holding the directory it ran in,
//! with each seat's session and state, the sessions of seats its config
//! dropped, its spawned sessions, and the seats backing off; or, when no fleet holds it, every
//! fleet and the sessions of none. States come through observe.

use std::collections::BTreeMap;
use std::path::Path;

use fleet_core::agent::SessionState;
use fleet_core::messages::Recipient;
use fleet_core::protocol::{Reply, Response, response};
use fleet_core::seats::{Roster, SeatKey};
use state::{FleetRoster, Life, MessageRow, SessionRow, now_millis};

use crate::tending::Down;
use crate::{Error, Supervisor};

/// The overview from `directory`: the fleet holding it, or every fleet
/// and the loose sessions.
pub(crate) fn overview(sup: &Supervisor, directory: &Path) -> Result<Reply, Error> {
    let (here, rosters, rows, held) = {
        let s = sup.lock();
        let here = crate::names::here(&s, directory)?.map(|f| f.directory);
        (here, s.rosters()?, s.sessions()?, s.held()?)
    };
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
    let observed = match crate::observe::observe(&sup.tools, &shown, now_millis()) {
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
    let down = sup.tending.down();
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
                .filter(|r| crate::names::of_seat(r, &f.fleet, &seat.name))
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

/// One seat: its session among `rows`, oldest launched first (the live
/// one, else the newest), whether that was
/// launched from an older role, and its backoff while nothing of it lives.
fn seat_overview(
    roster: &Roster,
    seat: &fleet_core::seats::Seat,
    rows: &[&SessionRow],
    down: Option<&Down>,
    listed: &impl Fn(&SessionRow) -> response::Listed,
) -> response::SeatOverview {
    let chosen = rows
        .iter()
        .find(|r| r.life == Life::Live)
        .or_else(|| rows.last());
    let live = chosen.is_some_and(|r| r.life == Life::Live);
    let digest = roster.role(&seat.role).map(fleet_core::seats::Role::digest);
    response::SeatOverview {
        name: seat.name.clone(),
        role: seat.role.clone(),
        older_role: chosen.is_some_and(|r| r.role_digest != digest),
        session: chosen.map(|r| listed(r)),
        down: down.filter(|_| !live).map(|d| response::Down {
            why: d.why.clone(),
            next_try_millis: d.backoff.next_try_millis,
        }),
        waiting: 0,
    }
}
