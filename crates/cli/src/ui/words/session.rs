//! The words for agents' sessions: spawned, ended, and what goes wrong.

use fleet_core::agent::{AdapterFault, Verb};
use fleet_core::protocol::response;
use fleet_core::status::Session;

use crate::ui::output::Ink;

/// What `fleet spawn` did: the id is the second word, for a script.
pub(super) fn spawned(spawned: &response::Spawn, ink: Ink) -> String {
    let response::Spawn::Spawned {
        session,
        role,
        directory,
        ..
    } = spawned;
    format!(
        "spawned {} from {} in {}",
        ink.accent(session),
        ink.accent(role),
        ink.path(directory)
    )
}

/// What `fleet end` did.
pub(super) fn ended(ended: &response::End, ink: Ink) -> String {
    match ended {
        response::End::Ended {
            session,
            seat: None,
        } => format!("ended {}", ink.accent(session)),
        response::End::Ended {
            session,
            seat: Some(seat),
        } => format!(
            "ended {}'s session {}; {} comes back fresh while its fleet is on",
            ink.accent(seat),
            ink.accent(session),
            ink.accent(seat)
        ),
    }
}

/// Why the supervisor said no to a session request.
pub(super) fn refused(refusal: &Session, ink: Ink) -> String {
    match refusal {
        Session::AdapterFailed {
            adapter,
            verb,
            fault,
        } => format!(
            "the adapter {} failed to {}: {}",
            ink.path(adapter),
            match verb {
                Verb::Launch => "launch the agent",
                Verb::Read => "read its sessions",
                Verb::Protocol => "say which protocol it speaks",
            },
            fault_words(fault)
        ),
        Session::MultiplexerCouldNotRun { error } => {
            format!("{} could not be run: {error}", ink.accent("tmux"))
        }
        Session::MultiplexerFailed { command, said } if said.is_empty() => {
            format!("{} failed", ink.accent(command))
        }
        Session::MultiplexerFailed { command, said } => {
            format!("{} failed: {said}", ink.accent(command))
        }
        Session::NoFleetHere { directory } => format!("no fleet holds {}", ink.path(directory)),
        Session::NoSuchRole { role, fleet } => format!(
            "the fleet in {} has no role {}",
            ink.path(fleet),
            ink.accent(role)
        ),
        Session::NotAProject { path, fleet } => format!(
            "{} is not one of the projects of the fleet in {}",
            ink.path(path),
            ink.path(fleet)
        ),
        Session::SeatNotRunning { seat, .. } => {
            format!("{} has no session running", ink.accent(seat))
        }
        Session::NoSuchSeatOrSession {
            name,
            fleet: Some(fleet),
        } => format!(
            "the fleet in {} has no seat {}, and no session has that id",
            ink.path(fleet),
            ink.accent(name)
        ),
        Session::NoSuchSeatOrSession { name, fleet: None } => {
            format!("no session has the id {}", ink.accent(name))
        }
        Session::LaunchTooLong { bytes, limit } => format!(
            "launching the agent takes a {bytes}-byte {} command, more than the {limit} it \
             takes: shorten its role's prompt",
            ink.accent("tmux")
        ),
    }
}

/// What to do about it.
pub(super) fn hint(refusal: &Session, ink: Ink) -> Option<String> {
    match refusal {
        Session::NoFleetHere { .. } => Some(format!(
            "run {} in a fleet's directory or one of its projects",
            ink.accent("fleet spawn")
        )),
        Session::NoSuchRole { fleet, .. } => Some(format!(
            "add the role to {}, then run {}",
            ink.path(&fleet.join(fleet_core::paths::CONFIG_FILE)),
            ink.accent("fleet start")
        )),
        Session::NotAProject { .. } => {
            Some("name a project's root, or leave the directory out for the role's own".into())
        }
        Session::NoSuchSeatOrSession { .. } => Some(format!(
            "run {} to see the seats and sessions there are",
            ink.accent("fleet status")
        )),
        Session::AdapterFailed { .. }
        | Session::MultiplexerCouldNotRun { .. }
        | Session::MultiplexerFailed { .. }
        | Session::LaunchTooLong { .. }
        | Session::SeatNotRunning { .. } => None,
    }
}

/// How an adapter failed, in words.
pub(super) fn fault_words(fault: &AdapterFault) -> String {
    match fault {
        AdapterFault::CouldNotRun { error } => format!("it could not be run: {error}"),
        AdapterFault::Exited { code, said } => {
            let how = match code {
                Some(code) => format!("it exited with {code}"),
                None => "a signal ended it".to_owned(),
            };
            if said.is_empty() {
                how
            } else {
                format!("{how}: {said}")
            }
        }
        AdapterFault::TimedOut { after_millis } if after_millis % 1000 == 0 => {
            format!("it did not answer within {} seconds", after_millis / 1000)
        }
        AdapterFault::TimedOut { after_millis } => {
            format!("it did not answer within {after_millis} ms")
        }
        AdapterFault::BadAnswer { error } => format!("its answer could not be read: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::OUT;
    use fleet_core::agent::{AdapterFault, Verb};
    use std::path::PathBuf;

    fn adapter_failed(fault: AdapterFault) -> String {
        let words = refused(
            &Session::AdapterFailed {
                adapter: PathBuf::from("/a/claude"),
                verb: Verb::Read,
                fault,
            },
            OUT.piped(),
        );
        console::strip_ansi_codes(&words).into_owned()
    }

    #[test]
    fn an_adapter_failure_says_which_adapter_what_it_was_asked_and_how_it_failed() {
        assert_eq!(
            adapter_failed(AdapterFault::Exited {
                code: Some(1),
                said: "claude: not found".into()
            }),
            "the adapter /a/claude failed to read its sessions: it exited with 1: claude: not found"
        );
        assert_eq!(
            adapter_failed(AdapterFault::TimedOut {
                after_millis: 20_000
            }),
            "the adapter /a/claude failed to read its sessions: it did not answer within 20 seconds"
        );
        assert_eq!(
            adapter_failed(AdapterFault::Exited {
                code: None,
                said: String::new()
            }),
            "the adapter /a/claude failed to read its sessions: a signal ended it"
        );
    }

    #[test]
    fn spawn_and_end_refusals_say_what_was_not_found() {
        let said = |s: &Session| console::strip_ansi_codes(&refused(s, OUT.piped())).into_owned();
        assert_eq!(
            said(&Session::NoFleetHere {
                directory: "/code/x".into()
            }),
            "no fleet holds /code/x"
        );
        assert_eq!(
            said(&Session::NoSuchRole {
                role: "worker".into(),
                fleet: "/code/wing".into()
            }),
            "the fleet in /code/wing has no role worker"
        );
        assert_eq!(
            said(&Session::NotAProject {
                path: "/code/app/src".into(),
                fleet: "/code/wing".into()
            }),
            "/code/app/src is not one of the projects of the fleet in /code/wing"
        );
        assert_eq!(
            said(&Session::SeatNotRunning {
                seat: "scribe".into(),
                fleet: "/code/app".into()
            }),
            "scribe has no session running"
        );
        assert_eq!(
            said(&Session::NoSuchSeatOrSession {
                name: "scrib".into(),
                fleet: Some("/code/app".into())
            }),
            "the fleet in /code/app has no seat scrib, and no session has that id"
        );
        assert_eq!(
            said(&Session::NoSuchSeatOrSession {
                name: "s0".into(),
                fleet: None
            }),
            "no session has the id s0"
        );
    }

    #[test]
    fn spawned_says_the_id_second_then_the_role_and_where() {
        let s = response::Spawn::Spawned {
            session: "s3fa01c2e9".into(),
            fleet: "/code/wing".into(),
            role: "worker".into(),
            directory: "/code/app".into(),
        };
        let said = console::strip_ansi_codes(&spawned(&s, OUT.piped())).into_owned();
        assert_eq!(said, "spawned s3fa01c2e9 from worker in /code/app");
        assert_eq!(said.split(' ').nth(1), Some("s3fa01c2e9"));
    }

    #[test]
    fn ended_says_when_a_seat_comes_back() {
        let plain =
            |e: &response::End| console::strip_ansi_codes(&ended(e, OUT.piped())).into_owned();
        assert_eq!(
            plain(&response::End::Ended {
                session: "s3fa01c2e9".into(),
                seat: None
            }),
            "ended s3fa01c2e9"
        );
        assert_eq!(
            plain(&response::End::Ended {
                session: "s1b2c3d4e5".into(),
                seat: Some("scribe".into())
            }),
            "ended scribe's session s1b2c3d4e5; scribe comes back fresh while its fleet is on"
        );
    }
}
