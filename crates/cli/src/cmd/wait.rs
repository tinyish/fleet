//! `fleet wait`: until a seat or session reads one of the states asked
//! for, or a message is typed in, and, given states, until its agent
//! reaches one of them after taking it. It polls the supervisor, each poll held
//! up to 25 s or the time left, and ends reached, refused when it can
//! never come, or timed out. It never starts the supervisor.

use std::path::Path;
use std::time::{Duration, Instant};

use fleet_core::protocol::{MAX_HOLD_MILLIS, Waited, request, response};

use crate::client;
use crate::protocol::{Problem, Response, Status, WaitState};
use crate::ui::output::{self, ERR, Mark};
use crate::ui::words;

/// Waits from the current directory for `name` to reach one of `states`,
/// or, for a message, to be delivered; at most `timeout` when given.
pub(crate) fn run(
    state_dir: &Path,
    name: &str,
    states: &[WaitState],
    timeout: Option<&str>,
) -> Result<Response, Status> {
    let timeout = timeout_of(timeout)?;
    let socket = client::socket(state_dir)?;
    if !client::answers(&socket) {
        return Err(Status::NotRunning);
    }
    let directory =
        std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    let deadline = timeout.map(|t| Instant::now() + t);
    let states: Vec<_> = states.iter().copied().map(WaitState::name).collect();
    // The first poll names what to wait on, and is not held, so the person
    // hears at once what it waits for; each later one carries what the
    // first resolved it to, and where to read from.
    let mut after: Option<(Waited, i64)> = None;
    loop {
        let hold = if after.is_some() {
            hold_for(deadline, Instant::now())
        } else {
            Duration::ZERO
        };
        let first = after.is_none();
        let hold_millis = u64::try_from(hold.as_millis()).unwrap_or(u64::MAX);
        let ask = match after.take() {
            None => request::Wait::For {
                directory: directory.clone(),
                name: name.to_owned(),
                states: states.clone(),
                hold_millis,
            },
            Some((on, from)) => request::Wait::On {
                on,
                states: states.clone(),
                from,
                hold_millis,
            },
        };
        match client::wait(&socket, ask, hold)? {
            response::Wait::Reached { on, session, state } => {
                return Ok(Response::Reached { on, session, state });
            }
            response::Wait::Delivered { message, session } => {
                return Ok(Response::Delivered { message, session });
            }
            response::Wait::Waiting {
                on,
                next,
                session,
                state,
            } => {
                if first {
                    output::line(
                        Mark::Plain,
                        &words::waiting(&on, session.as_deref(), &states, ERR),
                    );
                }
                if deadline.is_some_and(|d| Instant::now() >= d) {
                    return Err(Status::TimedOut {
                        on,
                        session,
                        state,
                        after: timeout.unwrap_or_default(),
                    });
                }
                after = Some((on, next));
            }
        }
    }
}

/// `--timeout`, as jiff reads a person's duration (`30s`, `5m`, `1h30m`);
/// a negative one is not a duration to wait.
fn timeout_of(given: Option<&str>) -> Result<Option<Duration>, Status> {
    let Some(given) = given else {
        return Ok(None);
    };
    given
        .parse::<jiff::SignedDuration>()
        .ok()
        .and_then(|d| Duration::try_from(d).ok())
        .map(Some)
        .ok_or_else(|| {
            Status::Cannot(Problem::BadTimeout {
                given: given.to_owned(),
            })
        })
}

/// How long the next poll may be held: the longest hold, or the time left.
fn hold_for(deadline: Option<Instant>, now: Instant) -> Duration {
    let longest = Duration::from_millis(MAX_HOLD_MILLIS);
    deadline.map_or(longest, |d| d.saturating_duration_since(now).min(longest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeout_reads_as_a_person_writes_it() {
        assert_eq!(
            timeout_of(Some("30s")).ok(),
            Some(Some(Duration::from_secs(30)))
        );
        assert_eq!(
            timeout_of(Some("5m")).ok(),
            Some(Some(Duration::from_mins(5)))
        );
        assert_eq!(
            timeout_of(Some("1h30m")).ok(),
            Some(Some(Duration::from_mins(90)))
        );
        assert_eq!(timeout_of(None).ok(), Some(None));
        assert!(matches!(
            timeout_of(Some("soon")),
            Err(Status::Cannot(Problem::BadTimeout { .. }))
        ));
        assert!(matches!(
            timeout_of(Some("-5m")),
            Err(Status::Cannot(Problem::BadTimeout { .. }))
        ));
    }

    #[test]
    fn each_poll_holds_at_most_the_time_left() {
        let start = Instant::now();
        assert_eq!(
            hold_for(None, start),
            Duration::from_millis(MAX_HOLD_MILLIS)
        );
        assert_eq!(
            hold_for(Some(start + Duration::from_secs(2)), start),
            Duration::from_secs(2)
        );
        assert_eq!(
            hold_for(Some(start), start + Duration::from_secs(1)),
            Duration::ZERO
        );
    }

    #[test]
    fn wait_never_starts_the_supervisor() {
        let dir = std::env::temp_dir().join(format!("fleet-wait-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(
            run(&dir, "scribe", &[WaitState::Idle], None),
            Err(Status::NotRunning)
        ));
        let socket = fleet_core::protocol::socket_path(&dir).ok();
        assert!(socket.is_some_and(|s| !s.exists()));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_bad_timeout_is_refused_before_the_supervisor_is_asked() {
        let dir = std::env::temp_dir().join(format!("fleet-wait-bad-{}", std::process::id()));
        assert!(matches!(
            run(&dir, "scribe", &[WaitState::Idle], Some("soon")),
            Err(Status::Cannot(Problem::BadTimeout { .. }))
        ));
    }
}
