//! Observe: each session's state, and the adapters that could not read
//! theirs. tmux says whether the agent's pane, found by the id tmux gave it
//! at launch, is alive; each agent's adapter, asked once for its live
//! sessions, says what the agent is doing (core.md § Primitives, States).

use std::collections::BTreeMap;
use std::path::Path;

use fleet_core::agent::{self, Reading, SessionRef, SessionState};
use fleet_core::protocol::response;
use fleet_core::tools::{Pane, ToolError};
use state::SessionRow;

use crate::Tools;

/// Each session's state, and the adapters that could not read theirs.
#[derive(Debug, Default)]
pub(crate) struct Observed {
    /// Each session's state, by its id.
    pub(crate) states: BTreeMap<String, SessionState>,
    /// The adapters that failed to read their sessions.
    pub(crate) faults: Vec<response::Fault>,
}

/// The state of each of `sessions` at `now_millis`: tmux lists the panes,
/// each session's pane is the one with its recorded id, in that session,
/// and each adapter is asked once, for its sessions whose pane is alive.
///
/// # Errors
///
/// When tmux cannot list its panes.
pub(crate) fn observe(
    tools: &Tools,
    sessions: &[SessionRow],
    now_millis: i64,
) -> Result<Observed, ToolError> {
    let panes = tools.mux.panes()?;
    let by_id = by_id(&panes);
    let pane_of = |row: &SessionRow| pane_of(&by_id, row);
    let mut asks: BTreeMap<&Path, Vec<SessionRef>> = BTreeMap::new();
    for row in sessions {
        if let Some(pane) = pane_of(row).filter(|p| !p.dead) {
            asks.entry(&row.adapter).or_default().push(SessionRef {
                session: row.id.clone(),
                pid: pane.pid,
                directory: row.directory.clone(),
            });
        }
    }
    let mut readings: BTreeMap<String, Reading> = BTreeMap::new();
    let mut faults = Vec::new();
    for (adapter, asked) in asks {
        let ids: Vec<String> = asked.iter().map(|s| s.session.clone()).collect();
        match tools.agents.read(adapter, &agent::Read { sessions: asked }) {
            Ok(answer) => readings.extend(
                answer
                    .sessions
                    .into_iter()
                    .filter(|r| ids.contains(&r.session))
                    .map(|r| (r.session.clone(), r)),
            ),
            Err(fault) => faults.push(response::Fault {
                adapter: adapter.to_path_buf(),
                fault,
            }),
        }
    }
    let states = sessions
        .iter()
        .map(|row| {
            let state = agent::state_of(
                pane_of(row),
                readings.get(&row.id),
                row.launch_time_millis,
                now_millis,
            );
            (row.id.clone(), state)
        })
        .collect();
    Ok(Observed { states, faults })
}

/// Whether the agent's pane of each of `sessions` is alive, by session id:
/// tmux alone, for the tending loop, which never asks an adapter to read.
///
/// # Errors
///
/// When tmux cannot list its panes.
pub(crate) fn alive(
    tools: &Tools,
    sessions: &[SessionRow],
) -> Result<BTreeMap<String, bool>, ToolError> {
    let panes = tools.mux.panes()?;
    let by_id = by_id(&panes);
    Ok(sessions
        .iter()
        .map(|row| {
            (
                row.id.clone(),
                pane_of(&by_id, row).is_some_and(|p| !p.dead),
            )
        })
        .collect())
}

/// `panes` by their ids.
fn by_id(panes: &[Pane]) -> BTreeMap<&str, &Pane> {
    panes.iter().map(|p| (p.id.as_str(), p)).collect()
}

/// The agent's pane of `row`: the one with its recorded id, in its session.
/// tmux numbers panes from %0 again when its server restarts, so a stale
/// row's id can name another session's pane: it must be this session's too.
fn pane_of<'a>(by_id: &BTreeMap<&str, &'a Pane>, row: &SessionRow) -> Option<&'a Pane> {
    by_id
        .get(row.pane.as_str())
        .copied()
        .filter(|p| p.session == row.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex, PoisonError};

    use fleet_core::agent::{Activity, AdapterFault, Reading, SessionState};
    use fleet_core::protocol::response;
    use fleet_core::tools::{Pane, ToolError};
    use state::SessionRow;

    use crate::Tools;
    use crate::testing::{FakeAgents, FakeMux, FakeNet, Shared};

    fn row(id: &str, adapter: &str, pane: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            adapter: PathBuf::from(adapter),
            directory: "/c/app".into(),
            launch_time_millis: 0,
            pane: pane.into(),
            fleet: None,
            seat: None,
            role: None,
            role_digest: None,
            state: None,
        }
    }

    fn pane(session: &str, id: &str, pid: u32, dead: bool) -> Pane {
        Pane {
            session: session.into(),
            id: id.into(),
            pid,
            dead,
        }
    }

    fn working(session: &str) -> Reading {
        Reading {
            session: session.into(),
            activity: Activity::Working,
            reason: None,
        }
    }

    fn tools(panes: Vec<Pane>, agents: Arc<FakeAgents>) -> Tools {
        Tools {
            mux: Box::new(FakeMux {
                panes: Mutex::new(panes),
                ..FakeMux::default()
            }),
            agents: Box::new(Shared(agents)),
            net: Box::new(FakeNet::default()),
        }
    }

    /// The pids each read asked about, per adapter, in order.
    fn asked(agents: &FakeAgents) -> Vec<(PathBuf, Vec<u32>)> {
        agents
            .reads
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .map(|(adapter, refs)| (adapter.clone(), refs.iter().map(|r| r.pid).collect()))
            .collect()
    }

    // A minute after launch: past the starting grace.
    const NOW: i64 = 60_000;

    #[test]
    fn a_split_off_pane_is_never_the_agents() {
        let agents = Arc::new(FakeAgents {
            readings: Some(BTreeMap::from([(
                PathBuf::from("/a/claude"),
                vec![working("s1")],
            )])),
            ..FakeAgents::default()
        });
        // The person's pane is listed last, as tmux lists a split.
        let t = tools(
            vec![pane("s1", "%4", 100, false), pane("s1", "%9", 900, false)],
            agents.clone(),
        );
        let seen = observe(&t, &[row("s1", "/a/claude", "%4")], NOW);
        let seen = seen.unwrap_or_else(|_| Observed::default());
        assert_eq!(
            asked(&agents),
            vec![(PathBuf::from("/a/claude"), vec![100])]
        );
        assert_eq!(seen.states.get("s1"), Some(&SessionState::Working));
    }

    #[test]
    fn the_agents_pane_ended_reads_gone_though_a_split_off_pane_lives() {
        let agents = Arc::new(FakeAgents::default());
        let t = tools(
            vec![pane("s1", "%4", 100, true), pane("s1", "%9", 900, false)],
            agents.clone(),
        );
        let seen = observe(&t, &[row("s1", "/a/claude", "%4")], NOW)
            .unwrap_or_else(|_| Observed::default());
        assert_eq!(seen.states.get("s1"), Some(&SessionState::Gone));
        assert_eq!(asked(&agents), vec![], "a gone session is not asked about");
        let t = tools(vec![pane("s1", "%9", 900, false)], agents.clone());
        let seen = observe(&t, &[row("s1", "/a/claude", "%4")], NOW)
            .unwrap_or_else(|_| Observed::default());
        assert_eq!(seen.states.get("s1"), Some(&SessionState::Gone));
    }

    #[test]
    fn a_pane_id_a_restarted_server_gave_another_session_is_not_this_ones() {
        // tmux numbers panes from %0 again after its server restarts: s1's
        // row is stale, and s2 now has the pane %0.
        let agents = Arc::new(FakeAgents::default());
        let t = tools(vec![pane("s2", "%0", 200, false)], agents.clone());
        let seen = observe(&t, &[row("s1", "/a/claude", "%0")], NOW)
            .unwrap_or_else(|_| Observed::default());
        assert_eq!(seen.states.get("s1"), Some(&SessionState::Gone));
        assert_eq!(asked(&agents), vec![], "a gone session is not asked about");
    }

    #[test]
    fn one_read_per_adapter_for_its_live_sessions_only() {
        let agents = Arc::new(FakeAgents::default());
        let t = tools(
            vec![
                pane("s1", "%1", 101, false),
                pane("s2", "%2", 102, true),
                pane("s3", "%3", 103, false),
            ],
            agents.clone(),
        );
        let rows = [
            row("s1", "/a/claude", "%1"),
            row("s2", "/a/claude", "%2"),
            row("s3", "/a/codex", "%3"),
        ];
        let seen = observe(&t, &rows, NOW).unwrap_or_else(|_| Observed::default());
        assert_eq!(
            asked(&agents),
            vec![
                (PathBuf::from("/a/claude"), vec![101]),
                (PathBuf::from("/a/codex"), vec![103])
            ]
        );
        assert_eq!(seen.states.get("s2"), Some(&SessionState::Gone));
        assert_eq!(seen.states.get("s1"), Some(&SessionState::Idle));
    }

    #[test]
    fn a_reading_about_a_session_not_asked_about_is_dropped() {
        // claude answers for s1 and, wrongly, for s2, which is codex's;
        // codex answers nothing, so s2 has no reading of its own.
        let agents = Arc::new(FakeAgents {
            readings: Some(BTreeMap::from([(
                PathBuf::from("/a/claude"),
                vec![working("s1"), working("s2"), working("s9")],
            )])),
            ..FakeAgents::default()
        });
        let t = tools(
            vec![pane("s1", "%1", 101, false), pane("s2", "%2", 102, false)],
            agents,
        );
        let rows = [row("s1", "/a/claude", "%1"), row("s2", "/a/codex", "%2")];
        let seen = observe(&t, &rows, NOW).unwrap_or_else(|_| Observed::default());
        assert_eq!(seen.states.get("s1"), Some(&SessionState::Working));
        assert_eq!(
            seen.states.get("s2"),
            Some(&SessionState::Unknown),
            "claude's reading of codex's session does not count"
        );
        assert_eq!(seen.states.get("s9"), None);
    }

    #[test]
    fn an_adapter_that_fails_is_a_fault_and_its_sessions_are_unknown() {
        let fault = AdapterFault::TimedOut {
            after_millis: 20_000,
        };
        let agents = Arc::new(FakeAgents {
            read_fault: Some(fault.clone()),
            ..FakeAgents::default()
        });
        let t = tools(vec![pane("s1", "%1", 101, false)], agents);
        let seen = observe(&t, &[row("s1", "/a/claude", "%1")], NOW)
            .unwrap_or_else(|_| Observed::default());
        assert_eq!(seen.states.get("s1"), Some(&SessionState::Unknown));
        assert_eq!(
            seen.faults,
            vec![response::Fault {
                adapter: PathBuf::from("/a/claude"),
                fault
            }]
        );
    }

    #[test]
    fn tmux_failing_is_the_error() {
        let t = Tools {
            mux: Box::new(FakeMux {
                missing: true,
                ..FakeMux::default()
            }),
            agents: Box::new(FakeAgents::default()),
            net: Box::new(FakeNet::default()),
        };
        assert!(matches!(
            observe(&t, &[row("s1", "/a/claude", "%1")], NOW),
            Err(ToolError::CouldNotRun { .. })
        ));
    }

    #[test]
    fn alive_asks_tmux_alone_and_matches_id_and_session() {
        let agents = Arc::new(FakeAgents::default());
        let t = tools(
            vec![
                pane("s1", "%1", 100, false),
                pane("s2", "%2", 200, true),
                pane("other", "%3", 300, false),
            ],
            agents.clone(),
        );
        let rows = [
            row("s1", "/a/claude", "%1"),
            row("s2", "/a/claude", "%2"),
            row("s3", "/a/claude", "%3"),
            row("s4", "/a/claude", "%4"),
        ];
        let got = alive(&t, &rows).unwrap_or_default();
        assert_eq!(
            got,
            BTreeMap::from([
                ("s1".to_owned(), true),
                ("s2".to_owned(), false),
                ("s3".to_owned(), false),
                ("s4".to_owned(), false),
            ])
        );
        assert_eq!(asked(&agents), vec![], "no adapter was asked");
    }
}
