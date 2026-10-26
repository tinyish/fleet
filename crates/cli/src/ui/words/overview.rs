//! What `fleet status` says: each fleet's seats and what their agents are
//! doing, the sessions of seats a config dropped, and the sessions of none.

use fleet_core::agent::{Reason, SessionState};
use fleet_core::protocol::response::{FleetOverview, Listed, Overview, SeatOverview};

use crate::ui::output::Ink;

/// The overview as lines: each fleet's heading, then its seats and orphans
/// indented under it; then the sessions of no fleet; then each adapter that
/// could not read. `now_millis` is when it was answered, by the CLI's clock.
pub(super) fn lines(o: &Overview, now_millis: i64, ink: Ink) -> String {
    let mut out: Vec<String> = Vec::new();
    for fleet in &o.fleets {
        out.push(format!(
            "{} · {}",
            ink.path(&fleet.directory),
            if fleet.on { "on" } else { "off" }
        ));
        out.extend(fleet_lines(fleet, now_millis, ink));
    }
    if !o.loose.is_empty() {
        out.push("not in a fleet".into());
        for l in &o.loose {
            out.push(
                format!(
                    "  {}  {}  {}",
                    ink.accent(&l.session),
                    state_word(l.state),
                    ink.path(&l.directory)
                )
                .trim_end()
                .to_owned(),
            );
        }
    }
    for f in &o.faults {
        out.push(format!(
            "{} could not read its sessions: {}",
            ink.path(&f.adapter),
            super::session::fault_words(&f.fault)
        ));
    }
    if out.is_empty() {
        return "there are no fleets yet".into();
    }
    out.join("\n")
}

/// One fleet's seat lines, each indented, in columns; then its orphans.
fn fleet_lines(fleet: &FleetOverview, now_millis: i64, ink: Ink) -> Vec<String> {
    if fleet.seats.is_empty() && fleet.orphans.is_empty() {
        return vec!["  no seats".into()];
    }
    let rows: Vec<[String; 4]> = fleet
        .seats
        .iter()
        .map(|seat| seat_row(seat, now_millis, ink))
        .collect();
    let width = |i: usize| {
        rows.iter()
            .map(|r| console::measure_text_width(&r[i]))
            .max()
            .unwrap_or(0)
    };
    let (w0, w1, w2) = (width(0), width(1), width(2));
    rows.iter()
        .map(|[a, b, c, d]| {
            format!("  {}  {}  {}  {d}", pad(a, w0), pad(b, w1), pad(c, w2))
                .trim_end()
                .to_owned()
        })
        .chain(fleet.orphans.iter().map(|o| {
            format!(
                "  orphaned  {}  was seat \"{}\"  {}",
                ink.accent(&o.session.session),
                o.seat,
                state_word(o.session.state)
            )
        }))
        .collect()
}

/// A seat's columns: its name, its role, its state, and the rest: its
/// session and whether it runs an older role, or why it is down and when
/// it is tried next.
fn seat_row(seat: &SeatOverview, now_millis: i64, ink: Ink) -> [String; 4] {
    let (state, rest) = match (&seat.down, &seat.session) {
        (Some(down), _) => {
            let next = match (down.next_try_millis - now_millis + 999) / 1000 {
                s if s <= 0 => "next try now".to_owned(),
                s => format!("next try in {s} s"),
            };
            let rest = match &down.why {
                Some(why) => format!("{} · {next}", super::refused(why, ink)),
                None => format!("it keeps ending · {next}"),
            };
            ("down".to_owned(), rest)
        }
        (None, Some(session)) => (state_word(session.state), session_rest(session, seat, ink)),
        (None, None) => ("not running".to_owned(), String::new()),
    };
    [ink.accent(&seat.name), seat.role.clone(), state, rest]
}

fn session_rest(session: &Listed, seat: &SeatOverview, ink: Ink) -> String {
    let id = ink.accent(&session.session);
    if seat.older_role {
        format!("{id}  launched from an older role")
    } else {
        id
    }
}

/// `text` padded with spaces to `width` columns, its colour not counted.
fn pad(text: &str, width: usize) -> String {
    let have = console::measure_text_width(text);
    format!("{text}{}", " ".repeat(width.saturating_sub(have)))
}

/// A session's state in a word or two.
fn state_word(state: SessionState) -> String {
    match state {
        SessionState::Starting => "starting".into(),
        SessionState::Working => "working".into(),
        SessionState::Idle => "idle".into(),
        SessionState::NeedsYou { reason: None } => "needs you".into(),
        SessionState::NeedsYou {
            reason: Some(reason),
        } => format!(
            "needs you: {}",
            match reason {
                Reason::Permission => "a permission prompt",
                Reason::Question => "a question",
                Reason::LoggedOut => "logged out",
                Reason::UsageLimit => "at its usage limit",
            }
        ),
        SessionState::Gone => "gone".into(),
        SessionState::Unknown => "unknown".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::OUT;
    use fleet_core::agent::{AdapterFault, Reason, SessionState};
    use fleet_core::protocol::response::{Down, FleetOverview, Listed, Orphan, SeatOverview};
    use std::path::PathBuf;

    fn listed(id: &str, state: SessionState) -> Listed {
        Listed {
            session: id.into(),
            adapter: PathBuf::from("/a/claude"),
            directory: PathBuf::from("/code/blog"),
            state,
            launch_time_millis: 0,
        }
    }

    fn seat(name: &str, session: Option<Listed>) -> SeatOverview {
        SeatOverview {
            name: name.into(),
            role: "writer".into(),
            session,
            older_role: false,
            down: None,
        }
    }

    const NOW: i64 = 1_000_000;

    fn plain(o: &Overview) -> String {
        console::strip_ansi_codes(&lines(o, NOW, OUT.piped())).into_owned()
    }

    #[test]
    fn a_fleet_shows_each_kind_of_seat() {
        let o = Overview {
            fleets: vec![FleetOverview {
                directory: PathBuf::from("/code/blog"),
                on: true,
                seats: vec![
                    seat("scribe", Some(listed("s1a2b3c4d5", SessionState::Idle))),
                    SeatOverview {
                        older_role: true,
                        ..seat("critic", Some(listed("s9f8e7d6c5", SessionState::Working)))
                    },
                    SeatOverview {
                        down: Some(Down {
                            why: Some(
                                fleet_core::status::Session::AdapterFailed {
                                    adapter: "/a/claude".into(),
                                    verb: fleet_core::agent::Verb::Launch,
                                    fault: AdapterFault::Exited {
                                        code: Some(1),
                                        said: "no".into(),
                                    },
                                }
                                .into(),
                            ),
                            next_try_millis: NOW + 40_000,
                        }),
                        ..seat("reader", None)
                    },
                    seat(
                        "asker",
                        Some(listed(
                            "s0",
                            SessionState::NeedsYou {
                                reason: Some(Reason::Permission),
                            },
                        )),
                    ),
                    seat("idler", None),
                ],
                orphans: vec![Orphan {
                    seat: "old".into(),
                    session: listed("s0011223344", SessionState::Idle),
                }],
            }],
            ..Overview::default()
        };
        let text = plain(&o);
        for want in [
            "/code/blog",
            "· on",
            "scribe",
            "idle",
            "s1a2b3c4d5",
            "critic",
            "working",
            "older role",
            "reader",
            "down",
            "exited with 1",
            "next try in 40 s",
            "needs you",
            "permission",
            "not running",
            "orphaned",
            "was seat \"old\"",
            "s0011223344",
        ] {
            assert!(text.contains(want), "{want:?} in\n{text}");
        }
        let seat_lines: Vec<&str> = text.lines().skip(1).collect();
        assert!(seat_lines.iter().all(|l| l.starts_with("  ")), "{text}");
    }

    #[test]
    fn every_fleet_then_the_loose_sessions() {
        let o = Overview {
            fleets: vec![FleetOverview {
                directory: PathBuf::from("/code/wing"),
                on: false,
                seats: vec![],
                orphans: vec![],
            }],
            loose: vec![listed("s77", SessionState::Gone)],
            faults: vec![],
        };
        let text = plain(&o);
        assert!(text.contains("/code/wing · off"), "{text}");
        assert!(text.contains("not in a fleet"), "{text}");
        assert!(text.contains("s77") && text.contains("gone"), "{text}");
    }
}
