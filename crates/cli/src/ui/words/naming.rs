//! The words for a name that points at nothing a command takes.

use fleet_core::status::{Naming, Takes};

use crate::ui::output::Ink;

/// Why the supervisor refused a name: where it looked, for just the kinds
/// the command takes.
pub(super) fn refused(refusal: &Naming, ink: Ink) -> String {
    let Naming::Unknown { name, fleet, takes } = refusal;
    let ids = match takes {
        Takes::SeatOrSession => "session",
        Takes::SeatSessionOrMessage => "session or message",
    };
    match fleet {
        Some(fleet) => format!(
            "the fleet in {} has no seat {}, and no {ids} has that id",
            ink.path(fleet),
            ink.accent(name)
        ),
        None => format!("no {ids} has the id {}", ink.accent(name)),
    }
}

/// What to do about it.
pub(super) fn hint(refusal: &Naming, ink: Ink) -> String {
    match refusal {
        Naming::Unknown { .. } => format!(
            "run {} to see the seats and sessions there are",
            ink.accent("fleet status")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::ERR;
    use std::path::PathBuf;

    fn said(name: &str, fleet: Option<&str>, takes: Takes) -> String {
        let refusal = Naming::Unknown {
            name: name.into(),
            fleet: fleet.map(PathBuf::from),
            takes,
        };
        console::strip_ansi_codes(&refused(&refusal, ERR.piped())).into_owned()
    }

    #[test]
    fn an_unknown_name_says_where_it_looked_for_what_the_command_takes() {
        assert_eq!(
            said("scrib", Some("/code/app"), Takes::SeatOrSession),
            "the fleet in /code/app has no seat scrib, and no session has that id"
        );
        assert_eq!(
            said("s0", None, Takes::SeatOrSession),
            "no session has the id s0"
        );
        assert_eq!(
            said("critic", Some("/code/app"), Takes::SeatSessionOrMessage),
            "the fleet in /code/app has no seat critic, and no session or message has that id"
        );
        assert_eq!(
            said("m0", None, Takes::SeatSessionOrMessage),
            "no session or message has the id m0"
        );
    }
}
