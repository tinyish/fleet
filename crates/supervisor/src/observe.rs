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
