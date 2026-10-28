//! The tending loop (core.md § Primitives, Seats): one thread that keeps
//! every seat of a fleet with a project on filled. It ticks every 2 s, and
//! at once when start, create or a message pokes it; a tick reads what is
//! wanted from state, asks tmux what is alive, and launches what is
//! missing, backing a seat off when it keeps failing; then it reads every
//! session once, records each state that changed, and delivers the
//! messages waiting for idle agents from that reading. It never ends a
//! session.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Condvar, Mutex, PoisonError};
use std::time::{Duration, Instant};

use fleet_core::agent::SessionState;
use fleet_core::messages::Hold;
use fleet_core::protocol::Status;
use fleet_core::seats::{self, Act, Backoff, Held, SeatKey, Tended, Wanted};
use state::{SessionRow, State, Step};

use crate::observe::Observed;

use crate::adapters::Store;
use crate::sessions::{self, Ask, ForSeat, Started};
use crate::{Tools, lock, log, words};

/// How long the loop waits between ticks when nobody pokes it.
const EVERY: Duration = Duration::from_secs(2);

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
    /// The sessions that took a paste and take no more yet (core.md
    /// invariant 12); only the loop's own thread changes them.
    holds: Mutex<BTreeMap<String, Hold>>,
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

    /// The sessions held after a paste.
    pub(crate) fn holds(&self) -> BTreeMap<String, Hold> {
        self.holds
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// Keeps `holds` as the sessions held after a paste.
    pub(crate) fn set_holds(&self, holds: BTreeMap<String, Hold>) {
        *self.holds.lock().unwrap_or_else(PoisonError::into_inner) = holds;
    }

    fn set_down(&self, key: &SeatKey, down: Option<Down>) {
        let mut all = self.down.lock().unwrap_or_else(PoisonError::into_inner);
        match down {
            Some(down) => all.insert(key.clone(), down),
            None => all.remove(key),
        };
    }

    /// Waits for a poke, or [`EVERY`].
    fn wait(&self) {
        let poked = self.poked.lock().unwrap_or_else(PoisonError::into_inner);
        let (mut poked, _) = self
            .bell
            .wait_timeout_while(poked, EVERY, |p| !*p)
            .unwrap_or_else(PoisonError::into_inner);
        *poked = false;
    }
}

/// Ticks forever.
pub(crate) fn run(
    state: &Arc<Mutex<State>>,
    tools: &Arc<Tools>,
    store: &Arc<Store>,
    tending: &Arc<Tending>,
) {
    loop {
        tick(state, tools, store, tending, state::now_millis());
        tending.wait();
    }
}

/// One tick at `now_millis`: the seats tended, then what waits for an idle
/// agent delivered, whatever is on or off. Delivery's clock is moved on by
/// the time tending took, so a hold starts when its paste goes in.
pub(crate) fn tick(
    state: &Mutex<State>,
    tools: &Tools,
    store: &Store,
    tending: &Tending,
    now_millis: i64,
) {
    let started = Instant::now();
    tend_seats(state, tools, store, tending, now_millis);
    let spent = i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX);
    let now_millis = now_millis.saturating_add(spent);
    let rows = match lock(state).sessions() {
        Ok(rows) => rows,
        Err(e) => return log(&words::tending_failed(&e.into())),
    };
    // tmux unable to list its panes: nothing is known, so nothing is
    // recorded or typed; tend_seats has said so.
    let Ok(observed) = crate::observe::observe(tools, &rows, now_millis) else {
        return;
    };
    record_states(state, &rows, &observed, &tending.holds(), now_millis);
    crate::messages::deliver(state, tools, tending, &rows, &observed, now_millis);
}

/// Records each session whose state, as `observed`, differs from the one
/// last recorded on its row: in one write, its row and its step. A session
/// still idle under the hold of a paste is not recorded yet: its idle is
/// the answer only once its turn was seen, or the hold has passed.
fn record_states(
    state: &Mutex<State>,
    rows: &[SessionRow],
    observed: &Observed,
    holds: &BTreeMap<String, Hold>,
    now_millis: i64,
) {
    let held = |r: &SessionRow| holds.get(&r.id).is_some_and(|h| h.blocks(now_millis));
    let changed: Vec<(&SessionRow, SessionState)> = rows
        .iter()
        .filter_map(|r| observed.states.get(&r.id).map(|s| (r, *s)))
        .filter(|(r, s)| r.state != Some(*s))
        .filter(|(r, s)| !(*s == SessionState::Idle && held(r)))
        .collect();
    if changed.is_empty() {
        return;
    }
    let written = lock(state).write(|w| {
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
fn tend_seats(
    state: &Mutex<State>,
    tools: &Tools,
    store: &Store,
    tending: &Tending,
    now_millis: i64,
) {
    let read = {
        let s = lock(state);
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
    let alive = match crate::observe::alive(tools, &rows) {
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
        act(state, tools, store, tending, tended, now_millis);
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
fn act(
    state: &Mutex<State>,
    tools: &Tools,
    store: &Store,
    tending: &Tending,
    tended: Tended,
    now_millis: i64,
) {
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
    match sessions::start_seat(state, tools, store, &ask, &seat) {
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
            if let Err(e) = lock(state).write(|w| w.record(&failed)) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, PoisonError};

    use fleet_core::agent::{AdapterFault, SessionState};

    use crate::testing::{
        FakeAgents, FakeMux, FakeNet, Shared, SharedMux, crew, put, spawned_row, with_seat,
    };

    fn tools(mux: Arc<FakeMux>, agents: Arc<FakeAgents>) -> Tools {
        Tools {
            mux: Box::new(SharedMux(mux)),
            agents: Box::new(Shared(agents)),
            net: Box::new(FakeNet::default()),
        }
    }

    fn store(root: &std::path::Path) -> Store {
        Store::new(&root.join("st"), Some("https://r".into()))
    }

    fn steps(state: &Mutex<State>) -> Vec<String> {
        lock(state).steps().unwrap_or_default()
    }

    fn panes(mux: &FakeMux) -> usize {
        mux.panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    fn kill_all(mux: &FakeMux) {
        for p in mux
            .panes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter_mut()
        {
            p.dead = true;
        }
    }

    fn launches(agents: &FakeAgents) -> usize {
        agents
            .launches
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    #[test]
    fn a_tick_fills_a_seat_woken_with_its_role() {
        let (root, state, fleet) = with_seat("tend-fill");
        let mux = Arc::new(FakeMux::default());
        let agents = Arc::new(FakeAgents::default());
        let t = tools(mux.clone(), agents.clone());
        tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        let rows = lock(&state).sessions().unwrap_or_default();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].fleet.as_ref(), Some(&fleet.directory));
        assert_eq!(rows[0].seat.as_deref(), Some("scribe"));
        let digest = lock(&state)
            .roster(&fleet.directory)
            .ok()
            .and_then(|r| r.role("w").map(fleet_core::seats::Role::digest));
        assert_eq!(rows[0].role_digest, digest);
        assert_eq!(panes(&mux), 1);
        assert_eq!(
            agents
                .launches
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
            [PathBuf::from("/a/claude")]
        );
        assert!(steps(&state).contains(&"SESSION_LAUNCHED".to_owned()));
        // A second tick keeps it.
        tick(&state, &t, &store(&root), &Tending::new(), 2_000);
        assert_eq!(lock(&state).sessions().unwrap_or_default().len(), 1);
        assert_eq!(launches(&agents), 1);
    }

    #[test]
    fn a_seat_whose_pane_ended_is_cleared_then_launched_again() {
        let (root, state, _) = with_seat("tend-revive");
        let mux = Arc::new(FakeMux::default());
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        let tending = Tending::new();
        tick(&state, &t, &store(&root), &tending, 1_000);
        let first = lock(&state).sessions().unwrap_or_default();
        kill_all(&mux);
        tick(&state, &t, &store(&root), &tending, 2_000);
        let rows = lock(&state).sessions().unwrap_or_default();
        assert_eq!(
            rows.len(),
            1,
            "the gone session cleared, a new one in its place"
        );
        assert_ne!(rows[0].id, first[0].id);
        assert_eq!(panes(&mux), 1, "the dead session's tmux session ended");
        let s = steps(&state);
        let gone = s.iter().position(|x| x == "SESSION_GONE");
        let again = s.iter().rposition(|x| x == "SESSION_LAUNCHED");
        assert!(gone.is_some() && gone < again, "{s:?}");
    }

    #[test]
    fn a_seat_that_keeps_dying_waits_longer_between_revivals() {
        let (root, state, _) = with_seat("tend-dying");
        let mux = Arc::new(FakeMux::default());
        let agents = Arc::new(FakeAgents::default());
        let t = tools(mux.clone(), agents.clone());
        let tending = Tending::new();
        tick(&state, &t, &store(&root), &tending, 1_000);
        kill_all(&mux);
        tick(&state, &t, &store(&root), &tending, 2_000);
        assert_eq!(launches(&agents), 2, "the first revival is immediate");
        kill_all(&mux);
        for now in [3_000, 4_000, 5_000, 7_999] {
            tick(&state, &t, &store(&root), &tending, now);
        }
        assert_eq!(launches(&agents), 2, "the second waits 5 s");
        assert_eq!(
            tending.down().values().next().map(|d| d.backoff),
            Some(Backoff {
                failures: 2,
                next_try_millis: 8_000
            }),
            "the same death is counted once"
        );
        tick(&state, &t, &store(&root), &tending, 8_000);
        assert_eq!(launches(&agents), 3);
    }

    #[test]
    fn a_session_that_lived_ten_minutes_resets_the_backoff() {
        let (root, state, _) = with_seat("tend-steady");
        let mux = Arc::new(FakeMux::default());
        let agents = Arc::new(FakeAgents::default());
        let t = tools(mux.clone(), agents.clone());
        let tending = Tending::new();
        tick(&state, &t, &store(&root), &tending, 1_000);
        kill_all(&mux);
        tick(&state, &t, &store(&root), &tending, 2_000);
        assert!(!tending.down().is_empty());
        let launched = lock(&state).sessions().unwrap_or_default()[0].launch_time_millis;
        tick(
            &state,
            &t,
            &store(&root),
            &tending,
            launched + fleet_core::seats::STEADY_MILLIS,
        );
        assert!(tending.down().is_empty(), "{:?}", tending.down());
    }

    #[test]
    fn a_second_supervisor_adopts_instead_of_launching() {
        let (root, state, _) = with_seat("tend-adopt");
        let mux = Arc::new(FakeMux::default());
        let agents = Arc::new(FakeAgents::default());
        let t = tools(mux.clone(), agents.clone());
        tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        // A fresh Tending is a restarted supervisor: same state, same tmux.
        tick(&state, &t, &store(&root), &Tending::new(), 5_000);
        assert_eq!(panes(&mux), 1);
        assert_eq!(launches(&agents), 1);
        assert_eq!(lock(&state).sessions().unwrap_or_default().len(), 1);
    }

    #[test]
    fn off_launches_nothing_and_ends_nothing() {
        let (root, state, _) = with_seat("tend-off");
        let mux = Arc::new(FakeMux::default());
        let agents = Arc::new(FakeAgents::default());
        let t = tools(mux.clone(), agents.clone());
        assert!(lock(&state).write(|w| w.set_all_active(false)).is_ok());
        tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        assert_eq!(launches(&agents), 0, "an off fleet's seat is not filled");
        assert!(lock(&state).write(|w| w.set_all_active(true)).is_ok());
        tick(&state, &t, &store(&root), &Tending::new(), 2_000);
        assert!(lock(&state).write(|w| w.set_all_active(false)).is_ok());
        kill_all(&mux);
        tick(&state, &t, &store(&root), &Tending::new(), 3_000);
        assert_eq!(panes(&mux), 1, "nothing launched, nothing ended");
        assert_eq!(launches(&agents), 1);
        assert_eq!(
            lock(&state).sessions().unwrap_or_default().len(),
            1,
            "nothing cleared"
        );
    }

    #[test]
    fn a_removed_seats_session_is_left_running() {
        let (root, state, fleet) = with_seat("tend-orphan");
        let mux = Arc::new(FakeMux::default());
        let agents = Arc::new(FakeAgents::default());
        let t = tools(mux.clone(), agents.clone());
        tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        let empty = fleet_core::seats::Roster::default();
        assert!(
            lock(&state)
                .write(|w| w.set_roster(&fleet.directory, &empty))
                .is_ok()
        );
        kill_all(&mux);
        tick(&state, &t, &store(&root), &Tending::new(), 2_000);
        assert_eq!(launches(&agents), 1, "never revived");
        assert_eq!(lock(&state).sessions().unwrap_or_default().len(), 1);
    }

    #[test]
    fn a_failing_launch_backs_off_and_shows_down() {
        let (root, state, _) = with_seat("tend-fail");
        let agents = Arc::new(FakeAgents {
            launch_fault: Some(AdapterFault::Exited {
                code: Some(1),
                said: "no".into(),
            }),
            ..FakeAgents::default()
        });
        let t = tools(Arc::new(FakeMux::default()), agents.clone());
        let tending = Tending::new();
        tick(&state, &t, &store(&root), &tending, 1_000);
        tick(&state, &t, &store(&root), &tending, 1_000);
        tick(&state, &t, &store(&root), &tending, 2_000);
        assert_eq!(
            launches(&agents),
            2,
            "once, again at once, then not before 5 s"
        );
        let down = tending.down().values().next().cloned();
        assert!(
            down.as_ref()
                .is_some_and(|d| d.why.is_some() && d.backoff.next_try_millis == 6_000),
            "{:?}",
            down.map(|d| d.backoff)
        );
        assert_eq!(
            steps(&state)
                .iter()
                .filter(|s| *s == "SEAT_LAUNCH_FAILED")
                .count(),
            2
        );
        assert_eq!(lock(&state).sessions().unwrap_or_default().len(), 0);
    }

    #[test]
    fn a_tmux_that_cannot_list_launches_nothing_and_says_why() {
        let (root, state, _) = with_seat("tend-notmux");
        let mux = Arc::new(FakeMux {
            missing: true,
            ..FakeMux::default()
        });
        let agents = Arc::new(FakeAgents::default());
        let tending = Tending::new();
        tick(
            &state,
            &tools(mux, agents.clone()),
            &store(&root),
            &tending,
            1_000,
        );
        assert_eq!(launches(&agents), 0);
        let down = tending.down();
        assert_eq!(down.len(), 1);
        assert!(down.values().all(|d| d.why.is_some()));
        assert!(!steps(&state).iter().any(|s| s == "SEAT_LAUNCH_FAILED"));
        // tmux back: the seat is filled at once.
        let t = tools(Arc::new(FakeMux::default()), agents.clone());
        tick(&state, &t, &store(&root), &tending, 1_500);
        assert_eq!(launches(&agents), 1);
    }

    #[test]
    fn a_poke_wakes_the_loop_before_its_tick() {
        let tending = Arc::new(Tending::new());
        let waiter = Arc::clone(&tending);
        let woke = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            waiter.wait();
            started.elapsed()
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        tending.poke();
        let took = woke.join().unwrap_or(EVERY);
        assert!(took < EVERY, "{took:?}");
    }

    /// The states recorded, in order, as (session, state).
    fn recorded(state: &Mutex<State>) -> Vec<(String, SessionState)> {
        lock(state)
            .events(1, None, 1 << 20)
            .unwrap_or_default()
            .into_iter()
            .filter_map(|e| match e.step {
                Step::SessionStateChanged { session, state, .. } => Some((session, state)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_state_is_recorded_once_when_it_changes() {
        let (root, state, _) = with_seat("record-once");
        let mux = Arc::new(FakeMux::default());
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        let tending = Tending::new();
        tick(&state, &t, &store(&root), &tending, 1_000);
        tick(&state, &t, &store(&root), &tending, 2_000);
        let id = lock(&state).sessions().unwrap_or_default()[0].id.clone();
        assert_eq!(recorded(&state), vec![(id.clone(), SessionState::Idle)]);
        kill_all(&mux);
        tick(&state, &t, &store(&root), &tending, 3_000);
        // Tending clears the seat's gone session and relaunches it in the
        // same tick (SESSION_GONE), so no gone state is recorded for it;
        // the new session's idle is.
        let after = recorded(&state);
        assert_eq!(after.len(), 2, "{after:?}");
        assert_ne!(after[1].0, id);
        assert_eq!(after[1].1, SessionState::Idle);
    }

    #[test]
    fn a_restarted_supervisor_records_nothing_that_did_not_change() {
        let (root, state, _) = with_seat("record-restart");
        let t = tools(
            Arc::new(FakeMux::default()),
            Arc::new(FakeAgents::default()),
        );
        tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        tick(&state, &t, &store(&root), &Tending::new(), 2_000);
        assert_eq!(recorded(&state).len(), 1);
    }

    #[test]
    fn a_spawned_session_going_gone_is_recorded() {
        let (root, state, wing) = crew("record-spawned", &[("w", "/a/claude")]);
        let mux = Arc::new(FakeMux::default());
        put(&state, mux.as_ref(), &spawned_row("s1", &wing, 0));
        let t = tools(mux.clone(), Arc::new(FakeAgents::default()));
        tick(&state, &t, &store(&root), &Tending::new(), 60_000);
        kill_all(&mux);
        tick(&state, &t, &store(&root), &Tending::new(), 62_000);
        assert_eq!(
            recorded(&state),
            vec![
                ("s1".into(), SessionState::Idle),
                ("s1".into(), SessionState::Gone)
            ]
        );
    }

    #[test]
    fn a_tmux_that_cannot_list_records_no_state() {
        let (root, state, _) = with_seat("record-blind");
        let mux = Arc::new(FakeMux {
            missing: true,
            ..FakeMux::default()
        });
        let t = tools(mux, Arc::new(FakeAgents::default()));
        tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        assert_eq!(recorded(&state), []);
    }

    #[test]
    fn each_adapter_is_read_once_a_tick_for_every_live_session() {
        let (root, state, _) = with_seat("read-once");
        let agents = Arc::new(FakeAgents::default());
        let t = tools(Arc::new(FakeMux::default()), agents.clone());
        tick(&state, &t, &store(&root), &Tending::new(), 1_000);
        let reads = agents
            .reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len();
        assert_eq!(reads, 1);
    }

    #[test]
    fn a_paste_is_not_recorded_idle_until_the_turn_is_seen_or_the_hold_passes() {
        let (root, state, fleet) = with_seat("record-after-paste");
        let t = tools(
            Arc::new(FakeMux::default()),
            Arc::new(FakeAgents::default()),
        );
        let tending = Tending::new();
        tick(&state, &t, &store(&root), &tending, 1_000);
        tick(&state, &t, &store(&root), &tending, 2_000);
        let sent = crate::messages::send(
            &state,
            &t,
            &crate::messages::Send {
                directory: fleet.directory.clone(),
                to: "scribe".into(),
                text: "hi".into(),
                sender: fleet_core::messages::Sender::Script,
            },
        );
        assert!(sent.is_ok(), "{sent:?}");
        // This tick types it in; the fake agent reads idle throughout.
        tick(&state, &t, &store(&root), &tending, 3_000);
        let after_paste = || {
            let steps = steps(&state);
            let at = steps.iter().rposition(|s| s == "MESSAGE_DELIVERED");
            at.map(|at| steps[at + 1..].to_vec())
        };
        assert_eq!(after_paste(), Some(vec![]));
        tick(&state, &t, &store(&root), &tending, 5_000);
        assert_eq!(
            after_paste(),
            Some(vec![]),
            "idle under the hold is not the answer"
        );
        tick(
            &state,
            &t,
            &store(&root),
            &tending,
            3_000 + fleet_core::messages::HOLD_MILLIS + 1,
        );
        assert_eq!(
            after_paste(),
            Some(vec!["SESSION_STATE_CHANGED".to_owned()])
        );
    }
}
