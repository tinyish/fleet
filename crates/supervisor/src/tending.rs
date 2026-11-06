//! The tending loop (core.md § Primitives, Seats): one thread that keeps
//! every seat of a fleet with a project on filled. It ticks every 2 s, and
//! at once when start, create or a message pokes it; a tick reads what is
//! wanted from state, asks tmux what is alive, and launches what is
//! missing, backing a seat off when it keeps failing; then it reads every
//! session once, records each state that changed, and delivers the
//! messages waiting for idle agents from that reading. It never ends a
//! session.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use fleet_core::agent::SessionState;
use fleet_core::protocol::Status;
use fleet_core::seats::{self, Act, Backoff, Held, SeatKey, Tended, Wanted};
use state::{SessionRow, Step};

use crate::observe::Observed;

use crate::sessions::{self, Ask, ForSeat, Started};
use crate::{Supervisor, log, words};

/// How long the loop waits between ticks when nobody pokes it.
pub(crate) const EVERY: Duration = Duration::from_secs(2);

/// A seat backing off: its backoff, and why it last failed, when a launch
/// did (a session that kept ending has no reason).
#[derive(Debug, Clone, Default)]
pub(crate) struct Down {
    /// Its backoff.
    pub(crate) backoff: Backoff,
    /// Why its last launch failed, or why nothing could be launched.
    pub(crate) why: Option<Status>,
    /// Its ended sessions whose early end is already counted in `backoff`,
    /// so a session that stays ended through a wait counts once.
    counted: BTreeSet<String>,
}

/// The loop's own memory: the seats backing off, and its doorbell.
#[derive(Default)]
pub(crate) struct Tending {
    down: Mutex<BTreeMap<SeatKey, Down>>,
    /// Whether tmux could not list its panes last tick, so it is logged once.
    blind: Mutex<bool>,
    poked: Mutex<bool>,
    bell: Condvar,
}

impl Tending {
    /// A loop with nothing backing off, as a supervisor starts.
    pub(crate) fn new() -> Tending {
        Tending::default()
    }

    /// Wakes the loop for a tick now.
    pub(crate) fn poke(&self) {
        *self.poked.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.bell.notify_one();
    }

    /// The seats backing off.
    pub(crate) fn down(&self) -> BTreeMap<SeatKey, Down> {
        self.down
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set_down(&self, key: &SeatKey, down: Option<Down>) {
        let mut all = self.down.lock().unwrap_or_else(PoisonError::into_inner);
        match down {
            Some(down) => all.insert(key.clone(), down),
            None => all.remove(key),
        };
    }

    /// Waits for a poke, or [`EVERY`].
    pub(crate) fn wait(&self) {
        let poked = self.poked.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut poked, _) = self
            .bell
            .wait_timeout_while(poked, EVERY, |p| !*p)
            .unwrap_or_else(PoisonError::into_inner);
        *poked = false;
    }
}

/// One tick at `now_millis`: the seats tended, then every session read
/// once, the reading taken in by the paste holds, recorded, and what waits
/// for an idle agent delivered, whatever is on or off. Delivery's clock is
/// moved on by the time tending took, so a hold starts when its paste goes
/// in.
pub(crate) fn tick(sup: &Supervisor, now_millis: i64) {
    let started = Instant::now();
    tend_seats(sup, now_millis);
    let spent = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let now_millis = now_millis.saturating_add(spent);
    let rows = match sup.lock().sessions() {
        Ok(rows) => rows,
        Err(e) => return log(&words::tending_failed(&e.into())),
    };
    // tmux unable to list its panes: nothing is known, so nothing is
    // recorded or typed; tend_seats has said so.
    let Ok(observed) = crate::observe::observe(&sup.tools, &rows, now_millis) else {
        return;
    };
    let live = rows.iter().map(|r| r.id.as_str());
    sup.holds().saw(&observed.states, live, now_millis);
    record_states(sup, &rows, &observed, now_millis);
    crate::messages::deliver(sup, &rows, &observed, now_millis);
}

/// Records each session whose state, as `observed`, differs from the one
/// last recorded on its row: in one write, its row and its step. A session
/// still idle under the hold of a paste is not recorded yet: its idle is
/// the answer only once its turn was seen, or the hold has passed.
fn record_states(sup: &Supervisor, rows: &[SessionRow], observed: &Observed, now_millis: i64) {
    let holds = sup.holds();
    let changed: Vec<(&SessionRow, SessionState)> = rows
        .iter()
        .filter_map(|r| observed.states.get(&r.id).map(|s| (r, *s)))
        .filter(|(r, s)| r.state != Some(*s))
        .filter(|(r, s)| holds.counts(&r.id, *s, now_millis))
        .collect();
    drop(holds);
    if changed.is_empty() {
        return;
    }
    let written = sup.lock().write(|w| {
        for (row, now) in &changed {
            w.set_state(&row.id, Some(*now))?;
            w.record(&Step::SessionStateChanged {
                session: row.id.clone(),
                fleet: row.fleet.as_ref().map(|f| f.display().to_string()),
                seat: row.seat.clone(),
                state: *now,
            })?;
        }
        Ok(())
    });
    if let Err(e) = written {
        log(&words::tending_failed(&e.into()));
    }
}

/// Every wanted seat at `now_millis` kept, launched or left waiting.
fn tend_seats(sup: &Supervisor, now_millis: i64) {
    let tending = &sup.tending;
    let read = {
        let s = sup.lock();
        s.rosters().and_then(|r| s.sessions().map(|rows| (r, rows)))
    };
    let (rosters, rows) = match read {
        Ok(read) => read,
        Err(e) => return log(&words::tending_failed(&e.into())),
    };
    let wanted: Vec<Wanted> = rosters
        .into_iter()
        .filter(|f| f.on)
        .flat_map(|f| {
            let fleet = f.fleet;
            f.roster
                .seats
                .iter()
                .filter_map(|seat| {
                    f.roster.role(&seat.role).map(|role| Wanted {
                        key: SeatKey {
                            fleet: fleet.clone(),
                            seat: seat.name.clone(),
                        },
                        role: role.clone(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .collect();
    // A seat no longer wanted is not backing off: wanted again, it starts fresh.
    tending
        .down
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .retain(|key, _| wanted.iter().any(|w| w.key == *key));
    if wanted.is_empty() {
        return;
    }
    let alive = match crate::observe::alive(&sup.tools, &rows) {
        Ok(alive) => alive,
        Err(e) => return blind(tending, &wanted, &sessions::tool_status(e), now_millis),
    };
    *tending.blind.lock().unwrap_or_else(PoisonError::into_inner) = false;
    let held: Vec<Held> = rows
        .iter()
        .filter_map(|r| {
            Some(Held {
                session: r.id.clone(),
                key: SeatKey {
                    fleet: r.fleet.clone()?,
                    seat: r.seat.clone()?,
                },
                launch_time_millis: r.launch_time_millis,
                alive: alive.get(&r.id).copied().unwrap_or(false),
            })
        })
        .collect();
    let backoffs: BTreeMap<SeatKey, Backoff> = tending
        .down()
        .into_iter()
        .map(|(k, d)| (k, d.backoff))
        .collect();
    for tended in seats::tend(wanted, &held, &backoffs, now_millis) {
        act(sup, tended, now_millis);
    }
}

/// tmux could not list its panes, so nothing is known to be alive and
/// nothing is launched: launching blind could start a second session of a
/// live seat. Each wanted seat shows down with `why`; the log says so once.
fn blind(tending: &Tending, wanted: &[Wanted], why: &Status, now_millis: i64) {
    let mut down = tending.down.lock().unwrap_or_else(PoisonError::into_inner);
    for w in wanted {
        let mut d = down.get(&w.key).cloned().unwrap_or(Down {
            backoff: Backoff {
                failures: 0,
                next_try_millis: now_millis,
            },
            ..Down::default()
        });
        d.why = Some(why.clone());
        down.insert(w.key.clone(), d);
    }
    let mut was = tending.blind.lock().unwrap_or_else(PoisonError::into_inner);
    if !*was {
        log(&words::tmux_unlisted(why));
    }
    *was = true;
}

/// Does what `tended` says for its seat. A launch first counts its seat's
/// sessions that ended early, once each, and waits if that backs it off.
fn act(sup: &Supervisor, tended: Tended, now_millis: i64) {
    let tending = &sup.tending;
    let key = tended.wanted.key;
    let mut down = tending.down().remove(&key);
    let clear = match tended.act {
        Act::Wait | Act::Keep { steady: false } => return,
        Act::Keep { steady: true } => return tending.set_down(&key, None),
        Act::Launch { clear } => clear,
    };
    let early: Vec<String> = clear
        .iter()
        .filter(|h| seats::ended_early(h.launch_time_millis, now_millis))
        .filter(|h| {
            !down
                .as_ref()
                .is_some_and(|d| d.counted.contains(&h.session))
        })
        .map(|h| h.session.clone())
        .collect();
    if !early.is_empty() {
        let mut d = down.unwrap_or_default();
        d.backoff = Backoff::after_failure(Some(&d.backoff), now_millis);
        d.why = None;
        d.counted.extend(early);
        let waits = !d.backoff.allows(now_millis);
        down = Some(d);
        if waits {
            return tending.set_down(&key, down);
        }
    }
    let role = &tended.wanted.role;
    let ask = Ask {
        adapter: role.adapter.clone(),
        directory: role.directory.clone(),
        model: role.model.clone(),
        prompt: role.prompt.clone(),
    };
    let seat = ForSeat {
        key: key.clone(),
        role: role.label.clone(),
        role_digest: role.digest(),
        clear: clear.iter().map(|h| h.session.clone()).collect(),
    };
    match sessions::start_seat(sup, &ask, &seat) {
        Ok(Started::Launched) => tending.set_down(
            &key,
            down.map(|d| Down {
                backoff: d.backoff,
                ..Down::default()
            }),
        ),
        // The next tick decides again, from what is wanted then.
        Ok(Started::Unwanted) => tending.set_down(&key, down),
        Ok(Started::Refused(why)) => {
            let failed = Step::SeatLaunchFailed {
                fleet: key.fleet.display().to_string(),
                seat: key.seat.clone(),
                why: why.clone(),
            };
            if let Err(e) = sup.lock().write(|w| w.record(&failed)) {
                log(&words::tending_failed(&e.into()));
            }
            let mut d = down.unwrap_or_default();
            d.backoff = Backoff::after_failure(Some(&d.backoff), now_millis);
            d.why = Some(why);
            tending.set_down(&key, Some(d));
        }
        Err(e) => {
            log(&words::tending_failed(&e));
            tending.set_down(&key, down);
        }
    }
}
