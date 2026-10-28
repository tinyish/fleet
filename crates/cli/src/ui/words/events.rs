//! The words for `fleet events`: one line per recorded step, its id and
//! time, then what happened.

use std::path::Path;

use fleet_core::agent::SessionState;
use fleet_core::messages::{Recipient, Sender};
use fleet_core::protocol::Event;
use fleet_core::record::Step;

use crate::ui::output::Ink;

/// One event's line, at the machine's local time: its id, its time, then
/// what happened. `every` when every fleet is followed, so a seat is named
/// with its fleet.
pub(crate) fn line(event: &Event, every: bool, ink: Ink) -> String {
    line_at(event, every, &jiff::Zoned::now(), ink)
}

/// The line as seen at `now`, in `now`'s time zone.
fn line_at(event: &Event, every: bool, now: &jiff::Zoned, ink: Ink) -> String {
    format!(
        "{} {} {}",
        ink.muted(event.id),
        ink.muted(when(event.occur_time_millis, now)),
        sentence(&event.step, every, ink)
    )
}

/// What `fleet events` says as it starts, on stderr: the time, and what it
/// follows, from where.
pub(crate) fn listening(fleet: Option<&Path>, since: Option<i64>, ink: Ink) -> String {
    let r#where = match fleet {
        Some(fleet) => format!("in {}", ink.path(fleet)),
        None => "in every fleet".into(),
    };
    let what = match since {
        Some(id) => format!(
            "replaying events {where} from {}, then listening",
            ink.accent(id)
        ),
        None => format!("listening for events {where}"),
    };
    format!("{} {what}", ink.muted(clock()))
}

/// The time now, in the machine's zone: `HH:MM:SS`.
pub(super) fn clock() -> String {
    jiff::Zoned::now().strftime("%H:%M:%S").to_string()
}

/// The step's time in `now`'s zone: `HH:MM:SS`, after `Mon DD` when it is
/// not `now`'s day.
fn when(occur_time_millis: i64, now: &jiff::Zoned) -> String {
    let Ok(at) = jiff::Timestamp::from_millisecond(occur_time_millis) else {
        return "?".into();
    };
    let at = at.to_zoned(now.time_zone().clone());
    if at.date() == now.date() {
        at.strftime("%H:%M:%S").to_string()
    } else {
        at.strftime("%b %d %H:%M:%S").to_string()
    }
}

/// What happened, in a sentence. No `_` arm: a new step does not compile
/// until it has words here.
fn sentence(step: &Step, every: bool, ink: Ink) -> String {
    match step {
        Step::SupervisorStarted { version, pid } => {
            format!("the supervisor {version} started, pid {pid}")
        }
        Step::EveryFleetOn => "every fleet was turned on".into(),
        Step::EveryFleetOff => "every fleet was turned off".into(),
        Step::FleetCreated {
            directory,
            projects,
            adopted,
        } => created(directory, projects, *adopted, ink),
        Step::ProjectOn { project, .. } => format!("{} was turned on", path(project, ink)),
        Step::ProjectOff { project, .. } => format!("{} was turned off", path(project, ink)),
        Step::ProjectAdded { fleet, project } => {
            format!("{} joined {}", path(project, ink), the_fleet(fleet, ink))
        }
        Step::ProjectRemoved { fleet, project } => {
            format!("{} left {}", path(project, ink), the_fleet(fleet, ink))
        }
        Step::FleetConfigBroken { fleet, why } => format!(
            "{} stayed off: {}",
            the_fleet(fleet, ink),
            super::refused(why, ink)
        ),
        Step::FleetRemoved { directory, .. } => {
            format!("{} was removed", the_fleet(directory, ink))
        }
        Step::ProjectsTaken { fleet, projects } => format!(
            "{} {} taken out of {}",
            paths(projects, ink),
            if projects.len() == 1 { "was" } else { "were" },
            the_fleet(fleet, ink)
        ),
        Step::FleetForgotten { directory } => format!(
            "{} was forgotten: its directory is gone",
            the_fleet(directory, ink)
        ),
        step @ Step::SessionLaunched { .. } => launched(step, every, ink),
        Step::SessionEnded {
            session,
            fleet,
            seat,
        } => ended(session, fleet.as_deref(), seat.as_deref(), every, ink),
        Step::SessionStateChanged {
            session,
            fleet,
            seat,
            state,
        } => changed(
            session,
            fleet.as_deref(),
            seat.as_deref(),
            *state,
            every,
            ink,
        ),
        Step::SessionGone {
            session,
            fleet,
            seat,
        } => format!(
            "{} was gone, and was cleared to come back",
            who(session, Some(fleet), Some(seat), every, ink)
        ),
        Step::SeatAdded { seat, role, .. } => format!(
            "the seat {} was added, as {}",
            ink.accent(seat),
            ink.accent(role)
        ),
        Step::SeatRemoved { seat, .. } => format!("the seat {} was taken out", ink.accent(seat)),
        Step::SeatChanged { seat, role, .. } => {
            format!("{} now runs as {}", ink.accent(seat), ink.accent(role))
        }
        Step::RoleAdded { role, .. } => format!("the role {} was added", ink.accent(role)),
        Step::RoleRemoved { role, .. } => format!("the role {} was taken out", ink.accent(role)),
        Step::RoleChanged { role, .. } => format!("the role {} changed", ink.accent(role)),
        Step::SeatLaunchFailed { fleet, seat, why } => format!(
            "{} could not be launched: {}",
            seat_of(seat, Some(fleet), every, ink),
            super::refused(why, ink)
        ),
        Step::MessageQueued {
            message,
            to,
            sender,
            ..
        } => queued(message, to, sender, every, ink),
        Step::MessageDelivered {
            message, session, ..
        } => format!(
            "{} was delivered to {}",
            ink.accent(message),
            ink.accent(session)
        ),
        Step::MessageDropped { message, why, .. } => format!(
            "{} was dropped: {}",
            ink.accent(message),
            super::wait::dropped(*why)
        ),
    }
}

/// An ended session: a seat's names the seat.
fn ended(session: &str, fleet: Option<&str>, seat: Option<&str>, every: bool, ink: Ink) -> String {
    match seat {
        Some(seat) => format!(
            "{}'s session {} was ended",
            seat_of(seat, fleet, every, ink),
            ink.accent(session)
        ),
        None => format!("{} was ended", ink.accent(session)),
    }
}

fn queued(message: &str, to: &Recipient, sender: &Sender, every: bool, ink: Ink) -> String {
    format!(
        "{} was queued for {} by {}",
        ink.accent(message),
        recipient(to, every, ink),
        sent_by(sender, ink)
    )
}

fn created(directory: &str, projects: &[String], adopted: bool, ink: Ink) -> String {
    format!(
        "a fleet was created in {}, serving {}{}",
        path(directory, ink),
        paths(projects, ink),
        if adopted {
            ", from the fleet.toml there"
        } else {
            ""
        }
    )
}

/// A launch: a seat's names the seat; a spawned one its role and where it
/// works.
fn launched(step: &Step, every: bool, ink: Ink) -> String {
    let Step::SessionLaunched {
        session,
        directory,
        fleet,
        seat,
        role,
        ..
    } = step
    else {
        return String::new();
    };
    match (seat, role) {
        (Some(seat), _) => format!(
            "{} ({}) was launched",
            seat_of(seat, fleet.as_deref(), every, ink),
            ink.accent(session)
        ),
        (None, Some(role)) => format!(
            "{} was spawned from {} in {}",
            ink.accent(session),
            ink.accent(role),
            path(directory, ink)
        ),
        (None, None) => format!(
            "{} was launched in {}",
            ink.accent(session),
            path(directory, ink)
        ),
    }
}

/// A session's new state.
fn changed(
    session: &str,
    fleet: Option<&str>,
    seat: Option<&str>,
    state: SessionState,
    every: bool,
    ink: Ink,
) -> String {
    format!("{} {}", who(session, fleet, seat, every, ink), is(state))
}

/// A session: a seat's as `scribe (s1…)`, any other by its id.
fn who(session: &str, fleet: Option<&str>, seat: Option<&str>, every: bool, ink: Ink) -> String {
    match seat {
        Some(seat) => format!(
            "{} ({})",
            seat_of(seat, fleet, every, ink),
            ink.accent(session)
        ),
        None => ink.accent(session),
    }
}

/// A seat, with its fleet when every fleet is followed.
fn seat_of(seat: &str, fleet: Option<&str>, every: bool, ink: Ink) -> String {
    match fleet.filter(|_| every) {
        Some(fleet) => format!("{} of {}", ink.accent(seat), path(fleet, ink)),
        None => ink.accent(seat),
    }
}

/// A state after a session's name: `is idle`, or `needs you: …`.
fn is(state: SessionState) -> String {
    let word = super::overview::state_word(state);
    match state {
        SessionState::NeedsYou { .. } => word,
        _ => format!("is {word}"),
    }
}

fn recipient(to: &Recipient, every: bool, ink: Ink) -> String {
    match to {
        Recipient::Seat { fleet, seat } => {
            seat_of(seat, Some(&fleet.display().to_string()), every, ink)
        }
        Recipient::Session { session } => format!("the session {}", ink.accent(session)),
    }
}

fn sent_by(sender: &Sender, ink: Ink) -> String {
    match sender {
        Sender::Person => "a person".into(),
        Sender::Script => "a script".into(),
        Sender::Session { session } => ink.accent(session),
    }
}

/// The fleet in a directory.
fn the_fleet(directory: &str, ink: Ink) -> String {
    format!("the fleet in {}", path(directory, ink))
}

fn path(p: &str, ink: Ink) -> String {
    ink.path(Path::new(p))
}

fn paths(ps: &[String], ink: Ink) -> String {
    ps.iter()
        .map(|p| path(p, ink))
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::OUT;
    use fleet_core::agent::{Reason, SessionState};
    use fleet_core::protocol::Event;
    use fleet_core::record::Step;

    fn plain(text: &str) -> String {
        console::strip_ansi_codes(text).into_owned()
    }

    fn at(step: Step) -> Event {
        Event {
            id: 1043,
            occur_time_millis: 0,
            step,
        }
    }

    fn utc(millis: i64) -> Option<jiff::Zoned> {
        jiff::Timestamp::from_millisecond(millis)
            .ok()
            .map(|t| t.to_zoned(jiff::tz::TimeZone::UTC))
    }

    #[test]
    fn a_seats_state_names_the_seat_and_its_session() {
        let step = Step::SessionStateChanged {
            session: "s1a2b3c4d5".into(),
            fleet: Some("/code/app".into()),
            seat: Some("scribe".into()),
            state: SessionState::NeedsYou {
                reason: Some(Reason::Permission),
            },
        };
        assert_eq!(
            plain(&sentence(&step, false, OUT.piped())),
            "scribe (s1a2b3c4d5) needs you: a permission prompt"
        );
        assert_eq!(
            plain(&sentence(&step, true, OUT.piped())),
            "scribe of /code/app (s1a2b3c4d5) needs you: a permission prompt"
        );
    }

    #[test]
    fn a_spawned_sessions_state_names_it_alone() {
        let step = Step::SessionStateChanged {
            session: "s9".into(),
            fleet: Some("/w".into()),
            seat: None,
            state: SessionState::Idle,
        };
        assert_eq!(plain(&sentence(&step, false, OUT.piped())), "s9 is idle");
    }

    #[test]
    fn a_line_is_the_id_the_time_and_the_sentence() {
        let line =
            utc(0).map(|now| plain(&line_at(&at(Step::EveryFleetOn), false, &now, OUT.piped())));
        assert_eq!(
            line.as_deref(),
            Some("1043 00:00:00 every fleet was turned on")
        );
    }

    #[test]
    fn a_step_not_from_today_shows_its_date() {
        assert_eq!(
            utc(3 * 86_400_000).map(|now| when(0, &now)).as_deref(),
            Some("Jan 01 00:00:00")
        );
    }

    /// `line` without its leading time.
    fn after_time(line: &str) -> String {
        plain(line)
            .split_once(' ')
            .map(|(_, rest)| rest.to_owned())
            .unwrap_or_default()
    }

    #[test]
    fn events_says_what_it_follows_as_it_starts() {
        let app = std::path::Path::new("/code/app");
        assert_eq!(
            after_time(&listening(Some(app), None, OUT.piped())),
            "listening for events in /code/app"
        );
        assert_eq!(
            after_time(&listening(None, None, OUT.piped())),
            "listening for events in every fleet"
        );
        assert_eq!(
            after_time(&listening(Some(app), Some(1042), OUT.piped())),
            "replaying events in /code/app from 1042, then listening"
        );
    }
}
