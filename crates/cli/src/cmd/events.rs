//! `fleet events`: follows the fleet here, or every fleet, as it happens:
//! one line per recorded step, from now or from `--since`, until it is
//! stopped. It never starts the supervisor.

use std::io::{self, Write};
use std::path::Path;

use fleet_core::protocol::{Event, MAX_HOLD_MILLIS, request, response};

use crate::client;
use crate::protocol::{Doubt, Problem, Response, Status};
use crate::ui::output::{self, ERR, Mark, OUT};
use crate::ui::words;

/// Follows from `since`, or from now, until stdout closes or the
/// supervisor stops answering.
pub(crate) fn run(state_dir: &Path, since: Option<i64>) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    if !client::answers(&socket) {
        return Err(Status::NotRunning);
    }
    let directory =
        std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    let mut from = since;
    let mut out = io::stdout().lock();
    // The first poll is not held, so the person hears at once what is
    // followed.
    let mut first = true;
    loop {
        let follow = request::Events::Follow {
            directory: directory.clone(),
            from,
            hold_millis: if first { 0 } else { MAX_HOLD_MILLIS },
        };
        let response::Events::Steps {
            fleet,
            events,
            next,
        } = match (client::events(&socket, follow), from) {
            (Ok(steps), _) => steps,
            // Once something was asked, say where to go on from.
            (Err(Status::CouldNotTell(_)), Some(next)) => {
                return Err(Status::CouldNotTell(Doubt::Following { next }));
            }
            (Err(status), _) => return Err(status),
        };
        if first {
            output::line(Mark::Plain, &words::listening(fleet.as_deref(), since, ERR));
            first = false;
        }
        if print_all(&mut out, &events, fleet.is_none()).is_err() {
            return Ok(Response::Followed);
        }
        from = Some(next);
    }
}

/// Writes each event as its line, then flushes, so a pipe sees it at once.
/// Any write that fails means the reader went away.
fn print_all(out: &mut impl Write, events: &[Event], every: bool) -> io::Result<()> {
    for event in events {
        writeln!(out, "{}", words::event_line(event, every, OUT))?;
    }
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_core::protocol::Event;
    use fleet_core::record::Step;

    #[test]
    fn events_never_starts_the_supervisor() {
        let dir = std::env::temp_dir().join(format!("fleet-events-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        assert!(matches!(run(&dir, None), Err(Status::NotRunning)));
        let socket = fleet_core::protocol::socket_path(&dir).ok();
        assert!(socket.is_some_and(|s| !s.exists()));
        let _ = std::fs::remove_dir_all(dir);
    }

    /// A stdout whose reader went away.
    struct Closed;
    impl std::io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_closed_stdout_ends_events_quietly() {
        let mut out = Closed;
        assert!(print_all(&mut out, &[], false).is_ok());
        let one = vec![Event {
            id: 1,
            occur_time_millis: 0,
            step: Step::EveryFleetOn,
        }];
        assert!(print_all(&mut out, &one, false).is_err());
    }

    #[test]
    fn each_event_is_one_line() {
        let mut out = Vec::new();
        let two = vec![
            Event {
                id: 1,
                occur_time_millis: 0,
                step: Step::EveryFleetOn,
            },
            Event {
                id: 2,
                occur_time_millis: 0,
                step: Step::EveryFleetOff,
            },
        ];
        assert!(print_all(&mut out, &two, false).is_ok());
        let text = String::from_utf8_lossy(&out);
        assert_eq!(text.lines().count(), 2);
        assert!(text.ends_with("every fleet was turned off\n"), "{text}");
    }
}
