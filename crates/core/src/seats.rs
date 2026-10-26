//! Roles and seats (core.md § Primitives): what a fleet's config says about
//! the agents it runs, checked, with every path resolved; and which of them
//! the supervisor must launch, keep or leave waiting. Pure.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::adapters::{self, AdapterRef};
use crate::config::Config;
use crate::status::{self, Status};

/// How to run one kind of agent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Role {
    /// Its label in the config.
    pub label: String,
    /// The adapter, as a reference: an absolute path, or `<name>@<version>`.
    pub adapter: String,
    /// The model, in the agent's own naming; the adapter's default if none.
    pub model: Option<String>,
    /// What the agent is woken with.
    pub prompt: Option<String>,
    /// Where the agent works, absolute.
    pub directory: PathBuf,
}

/// An agent the supervisor keeps one session of running.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seat {
    /// Its name in the config.
    pub name: String,
    /// The label of its role.
    pub role: String,
}

/// A fleet's roles and seats, as its config last read, each sorted by its
/// label or name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Roster {
    /// Its roles.
    pub roles: Vec<Role>,
    /// Its seats; each names one of `roles`.
    pub seats: Vec<Seat>,
}

/// What reading a fleet's config again changed in its roster.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RosterChange {
    /// Seats it gained.
    pub added: Vec<Seat>,
    /// Names of seats it lost.
    pub removed: Vec<String>,
    /// Labels of roles still there whose digest moved.
    pub changed: Vec<String>,
    /// Seats still there that now run as another role, with it.
    pub moved: Vec<Seat>,
    /// Labels of roles it gained.
    pub roles_added: Vec<String>,
    /// Labels of roles it lost.
    pub roles_removed: Vec<String>,
}

impl Role {
    /// The SHA-256 of how it runs (adapter, model, prompt, directory), in
    /// hex: a session launched from it keeps this, so an edited role shows.
    #[must_use]
    pub fn digest(&self) -> String {
        let how = serde_json::json!([self.adapter, self.model, self.prompt, self.directory]);
        adapters::sha256_hex(how.to_string().as_bytes())
    }
}

impl Roster {
    /// The role labelled `label`.
    #[must_use]
    pub fn role(&self, label: &str) -> Option<&Role> {
        self.roles.iter().find(|r| r.label == label)
    }
}

/// Whether `name` may be a label or a seat's name: letters, digits, `-`
/// and `_`, starting with a letter or a digit.
fn good_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphanumeric())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The roster `config` describes for the fleet in `directory`, every path
/// resolved against it; `path` is the config file, for the refusals.
///
/// # Errors
///
/// A label or name that is not a good name, a seat whose role is not
/// there, or an adapter that is not a reference (a bare name included: the
/// latest is not looked up here).
pub fn roster_of(config: &Config, directory: &Path, path: &Path) -> Result<Roster, Status> {
    let bad_name = |name: &str| {
        Status::File(status::File::BadName {
            path: path.to_path_buf(),
            name: name.to_owned(),
        })
    };
    let mut roles = Vec::new();
    for (label, entry) in &config.roles {
        if !good_name(label) {
            return Err(bad_name(label));
        }
        let adapter = match adapters::parse_ref(&entry.adapter) {
            Ok(AdapterRef::Path(p)) => directory.join(p).display().to_string(),
            Ok(AdapterRef::Named(named)) => named.to_string(),
            Err(why) => {
                return Err(Status::File(status::File::RoleAdapter {
                    path: path.to_path_buf(),
                    role: label.clone(),
                    why: Box::new(why),
                }));
            }
        };
        roles.push(Role {
            label: label.clone(),
            adapter,
            model: entry.model.clone(),
            prompt: entry.prompt.clone(),
            directory: entry
                .directory
                .as_ref()
                .map_or_else(|| directory.to_path_buf(), |d| directory.join(d)),
        });
    }
    let mut seats = Vec::new();
    for (name, entry) in &config.seats {
        if !good_name(name) {
            return Err(bad_name(name));
        }
        if !config.roles.contains_key(&entry.role) {
            return Err(Status::File(status::File::UnknownRole {
                path: path.to_path_buf(),
                seat: name.clone(),
                role: entry.role.clone(),
            }));
        }
        seats.push(Seat {
            name: name.clone(),
            role: entry.role.clone(),
        });
    }
    Ok(Roster { roles, seats })
}

/// What `new` changes from `old`.
#[must_use]
pub fn diff(old: &Roster, new: &Roster) -> RosterChange {
    let had: BTreeMap<&str, &Seat> = old.seats.iter().map(|s| (s.name.as_str(), s)).collect();
    let has: BTreeMap<&str, &Seat> = new.seats.iter().map(|s| (s.name.as_str(), s)).collect();
    RosterChange {
        added: new
            .seats
            .iter()
            .filter(|s| !had.contains_key(s.name.as_str()))
            .cloned()
            .collect(),
        removed: old
            .seats
            .iter()
            .filter(|s| !has.contains_key(s.name.as_str()))
            .map(|s| s.name.clone())
            .collect(),
        changed: new
            .roles
            .iter()
            .filter(|r| old.role(&r.label).is_some_and(|o| o.digest() != r.digest()))
            .map(|r| r.label.clone())
            .collect(),
        moved: new
            .seats
            .iter()
            .filter(|s| had.get(s.name.as_str()).is_some_and(|o| o.role != s.role))
            .cloned()
            .collect(),
        roles_added: new
            .roles
            .iter()
            .filter(|r| old.role(&r.label).is_none())
            .map(|r| r.label.clone())
            .collect(),
        roles_removed: old
            .roles
            .iter()
            .filter(|r| new.role(&r.label).is_none())
            .map(|r| r.label.clone())
            .collect(),
    }
}

/// How long a session must live for its seat's backoff to reset, and the
/// longest a quick end still counts as a failure: ten minutes.
pub const STEADY_MILLIS: i64 = 600_000;
const FIRST_WAIT_MILLIS: i64 = 5_000;
const MOST_WAIT_MILLIS: i64 = 300_000;

/// One seat of one fleet.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SeatKey {
    /// The fleet's directory.
    pub fleet: PathBuf,
    /// The seat's name.
    pub seat: String,
}

/// A seat the supervisor must keep filled: one of a fleet with a project
/// on, with its role.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wanted {
    /// Which seat.
    pub key: SeatKey,
    /// How it runs.
    pub role: Role,
}

/// A session launched for a seat, and whether its pane is alive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    /// The session's id.
    pub session: String,
    /// The seat it was launched for.
    pub key: SeatKey,
    /// When it was launched.
    pub launch_time_millis: i64,
    /// Whether its agent's pane is alive.
    pub alive: bool,
}

/// What to do for one wanted seat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Act {
    /// Its session lives.
    Keep {
        /// Whether it has lived ten minutes, so its backoff resets.
        steady: bool,
    },
    /// Launch it, first clearing these ended sessions of it.
    Launch {
        /// Its sessions whose pane ended.
        clear: Vec<Held>,
    },
    /// It is backing off.
    Wait,
}

/// A wanted seat and what to do for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tended {
    /// The seat.
    pub wanted: Wanted,
    /// What to do.
    pub act: Act,
}

/// A seat's run of quick failures, and when it may be launched again.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Backoff {
    /// Quick failures in a row: failed launches, and sessions that ended early.
    pub failures: u32,
    /// When it may be launched again.
    pub next_try_millis: i64,
}

impl Backoff {
    /// The backoff after one more failure at `now_millis`: the first waits
    /// nothing, the next 5 s, each one after twice as long, at most 5 min.
    #[must_use]
    pub fn after_failure(prev: Option<&Backoff>, now_millis: i64) -> Backoff {
        let failures = prev.map_or(0, |b| b.failures).saturating_add(1);
        let wait = match failures {
            1 => 0,
            n => {
                let doublings = (n - 2).min(16);
                FIRST_WAIT_MILLIS
                    .saturating_mul(1_i64 << doublings)
                    .min(MOST_WAIT_MILLIS)
            }
        };
        Backoff {
            failures,
            next_try_millis: now_millis.saturating_add(wait),
        }
    }

    /// Whether the seat may be launched at `now_millis`.
    #[must_use]
    pub fn allows(&self, now_millis: i64) -> bool {
        now_millis >= self.next_try_millis
    }
}

/// Whether a session launched at `launch_time_millis` and gone by
/// `now_millis` ended early, which counts as a failure.
#[must_use]
pub fn ended_early(launch_time_millis: i64, now_millis: i64) -> bool {
    now_millis.saturating_sub(launch_time_millis) < STEADY_MILLIS
}

/// What to do for each of `wanted`, given the seats' sessions `held` and
/// their `backoff`: keep a seat whose session lives (adopting it, after a
/// restart); launch one with none alive once its backoff allows, clearing
/// its ended sessions; else wait. Sessions of seats not wanted are never
/// acted on.
#[must_use]
pub fn tend(
    wanted: Vec<Wanted>,
    held: &[Held],
    backoff: &BTreeMap<SeatKey, Backoff>,
    now_millis: i64,
) -> Vec<Tended> {
    wanted
        .into_iter()
        .map(|wanted| {
            let mine: Vec<&Held> = held.iter().filter(|h| h.key == wanted.key).collect();
            let act = if let Some(live) = mine.iter().find(|h| h.alive) {
                Act::Keep {
                    steady: now_millis.saturating_sub(live.launch_time_millis) >= STEADY_MILLIS,
                }
            } else if backoff
                .get(&wanted.key)
                .is_none_or(|b| b.allows(now_millis))
            {
                Act::Launch {
                    clear: mine.into_iter().cloned().collect(),
                }
            } else {
                Act::Wait
            };
            Tended { wanted, act }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config;
    use crate::status::{Adapter, File};

    fn read(text: &str) -> Result<Roster, Status> {
        let config = config::parse(text).map_err(|e| {
            Status::File(File::BadConfig {
                path: "/f/fleet.toml".into(),
                line: e.line,
                column: e.column,
                error: e.message,
            })
        })?;
        roster_of(&config, Path::new("/f"), Path::new("/f/fleet.toml"))
    }

    #[test]
    fn a_role_and_a_seat_read_with_paths_resolved_against_the_fleet() {
        let roster = read(
            "[roles.writer]\nadapter = \"bin/mine\"\nmodel = \"opus\"\ndirectory = \"notes\"\n\
             prompt = \"You write the notes.\"\n[seats.scribe]\nrole = \"writer\"\n",
        );
        assert_eq!(
            roster,
            Ok(Roster {
                roles: vec![Role {
                    label: "writer".into(),
                    adapter: "/f/bin/mine".into(),
                    model: Some("opus".into()),
                    prompt: Some("You write the notes.".into()),
                    directory: "/f/notes".into(),
                }],
                seats: vec![Seat {
                    name: "scribe".into(),
                    role: "writer".into()
                }],
            })
        );
    }

    #[test]
    fn a_role_defaults_to_the_fleet_directory_and_a_pinned_release() {
        let roles = read("[roles.w]\nadapter = \"claude-code@2.1.285\"\n").map(|r| r.roles);
        assert_eq!(
            roles,
            Ok(vec![Role {
                label: "w".into(),
                adapter: "claude-code@2.1.285".into(),
                model: None,
                prompt: None,
                directory: "/f".into(),
            }])
        );
    }

    #[test]
    fn an_absolute_directory_and_adapter_stay_as_written() {
        let roles =
            read("[roles.w]\nadapter = \"/opt/a\"\ndirectory = \"/elsewhere\"\n").map(|r| r.roles);
        assert_eq!(
            roles.map(|r| (r[0].adapter.clone(), r[0].directory.clone())),
            Ok(("/opt/a".to_owned(), PathBuf::from("/elsewhere")))
        );
    }

    #[test]
    fn a_seat_naming_no_role_is_refused() {
        assert_eq!(
            read("[seats.scribe]\nrole = \"writer\"\n"),
            Err(Status::File(File::UnknownRole {
                path: "/f/fleet.toml".into(),
                seat: "scribe".into(),
                role: "writer".into()
            }))
        );
    }

    #[test]
    fn a_name_with_other_characters_is_refused() {
        for bad in ["\"a b\"", "\"-a\"", "\"\"", "\"é\"", "\"x.y\""] {
            let seat = format!("[seats.{bad}]\nrole = \"w\"\n[roles.w]\nadapter = \"/a\"\n");
            let role = format!("[roles.{bad}]\nadapter = \"/a\"\n");
            for text in [seat, role] {
                assert!(
                    matches!(read(&text), Err(Status::File(File::BadName { .. }))),
                    "{text}"
                );
            }
        }
        assert!(read("[roles.w-1_A]\nadapter = \"/a\"\n[seats.9s]\nrole = \"w-1_A\"\n").is_ok());
    }

    #[test]
    fn an_unpinned_or_unreadable_adapter_is_refused_with_why() {
        assert_eq!(
            read("[roles.w]\nadapter = \"claude-code\"\n"),
            Err(Status::File(File::RoleAdapter {
                path: "/f/fleet.toml".into(),
                role: "w".into(),
                why: Box::new(Status::Adapter(Adapter::AdapterUnpinned {
                    name: "claude-code".into(),
                    latest: None
                })),
            }))
        );
        assert!(matches!(
            read("[roles.w]\nadapter = \"cc@latest\"\n"),
            Err(Status::File(File::RoleAdapter { .. }))
        ));
    }

    #[test]
    fn unknown_keys_are_refused() {
        assert!(config::parse("[roles.w]\nadapter = \"/a\"\npersistent = true\n").is_err());
        assert!(config::parse("[seats.s]\nrole = \"w\"\nmodel = \"x\"\n").is_err());
        assert!(
            config::parse("[seats.s]\n").is_err(),
            "a seat needs its role"
        );
        assert!(
            config::parse("[roles.w]\n").is_err(),
            "a role needs its adapter"
        );
    }

    #[test]
    fn the_digest_moves_with_any_field_and_only_then() {
        let r = Role {
            label: "w".into(),
            adapter: "/a".into(),
            model: None,
            prompt: Some("p".into()),
            directory: "/f".into(),
        };
        assert_eq!(r.digest(), r.clone().digest());
        assert_eq!(r.digest().len(), 64);
        let relabelled = Role {
            label: "v".into(),
            ..r.clone()
        };
        assert_eq!(
            r.digest(),
            relabelled.digest(),
            "the label is not how it runs"
        );
        for other in [
            Role {
                adapter: "/b".into(),
                ..r.clone()
            },
            Role {
                model: Some("p".into()),
                prompt: None,
                ..r.clone()
            },
            Role {
                prompt: Some("q".into()),
                ..r.clone()
            },
            Role {
                directory: "/g".into(),
                ..r.clone()
            },
        ] {
            assert_ne!(r.digest(), other.digest(), "{other:?}");
        }
    }

    #[test]
    fn the_diff_says_which_seats_came_went_and_which_roles_moved() {
        let old = read(
            "[roles.w]\nadapter = \"/a\"\n[roles.k]\nadapter = \"/k\"\n\
             [seats.s1]\nrole = \"w\"\n[seats.s2]\nrole = \"w\"\n",
        )
        .unwrap_or_default();
        let new = read(
            "[roles.w]\nadapter = \"/b\"\n[roles.k]\nadapter = \"/k\"\n[roles.n]\nadapter = \"/n\"\n\
             [seats.s2]\nrole = \"w\"\n[seats.s3]\nrole = \"w\"\n",
        )
        .unwrap_or_default();
        assert_eq!(
            diff(&old, &new),
            RosterChange {
                added: vec![Seat {
                    name: "s3".into(),
                    role: "w".into()
                }],
                removed: vec!["s1".to_owned()],
                changed: vec!["w".to_owned()],
                roles_added: vec!["n".to_owned()],
                ..RosterChange::default()
            }
        );
        assert_eq!(diff(&new, &new), RosterChange::default());
    }

    #[test]
    fn the_diff_says_which_seats_moved_role_and_which_roles_came_and_went() {
        let old = read(
            "[roles.w]\nadapter = \"/a\"\n[roles.gone]\nadapter = \"/g\"\n\
             [seats.s]\nrole = \"w\"\n",
        )
        .unwrap_or_default();
        let new = read(
            "[roles.w]\nadapter = \"/a\"\n[roles.k]\nadapter = \"/k\"\n\
             [seats.s]\nrole = \"k\"\n",
        )
        .unwrap_or_default();
        assert_eq!(
            diff(&old, &new),
            RosterChange {
                moved: vec![Seat {
                    name: "s".into(),
                    role: "k".into()
                }],
                roles_added: vec!["k".to_owned()],
                roles_removed: vec!["gone".to_owned()],
                ..RosterChange::default()
            }
        );
    }

    fn key(seat: &str) -> SeatKey {
        SeatKey {
            fleet: "/f".into(),
            seat: seat.into(),
        }
    }

    fn wanted(seat: &str) -> Wanted {
        Wanted {
            key: key(seat),
            role: Role {
                label: "w".into(),
                adapter: "/a".into(),
                model: None,
                prompt: None,
                directory: "/f".into(),
            },
        }
    }

    fn held(session: &str, seat: &str, alive: bool, launch: i64) -> Held {
        Held {
            session: session.into(),
            key: key(seat),
            launch_time_millis: launch,
            alive,
        }
    }

    fn acts(t: Vec<Tended>) -> Vec<(String, Act)> {
        t.into_iter().map(|t| (t.wanted.key.seat, t.act)).collect()
    }

    const NOW: i64 = 1_000_000;

    #[test]
    fn a_seat_with_no_session_is_launched() {
        let got = acts(tend(vec![wanted("s")], &[], &BTreeMap::new(), NOW));
        assert_eq!(got, vec![("s".into(), Act::Launch { clear: vec![] })]);
    }

    #[test]
    fn a_seat_whose_session_lives_is_kept_and_steady_after_ten_minutes() {
        let young = held("x1", "s", true, NOW - 1_000);
        let old = held("x1", "s", true, NOW - STEADY_MILLIS);
        assert_eq!(
            acts(tend(vec![wanted("s")], &[young], &BTreeMap::new(), NOW)),
            vec![("s".into(), Act::Keep { steady: false })]
        );
        assert_eq!(
            acts(tend(vec![wanted("s")], &[old], &BTreeMap::new(), NOW)),
            vec![("s".into(), Act::Keep { steady: true })]
        );
    }

    #[test]
    fn a_seat_whose_session_ended_is_cleared_and_launched() {
        let gone = held("x1", "s", false, 0);
        assert_eq!(
            acts(tend(
                vec![wanted("s")],
                std::slice::from_ref(&gone),
                &BTreeMap::new(),
                NOW
            )),
            vec![("s".into(), Act::Launch { clear: vec![gone] })]
        );
    }

    #[test]
    fn a_live_session_wins_over_a_dead_one_of_the_same_seat() {
        let live = held("x2", "s", true, NOW - 1);
        let dead = held("x1", "s", false, 0);
        assert_eq!(
            acts(tend(
                vec![wanted("s")],
                &[dead, live],
                &BTreeMap::new(),
                NOW
            )),
            vec![("s".into(), Act::Keep { steady: false })]
        );
    }

    #[test]
    fn a_seat_backing_off_waits_until_its_next_try() {
        let mut backoff = BTreeMap::new();
        backoff.insert(
            key("s"),
            Backoff {
                failures: 2,
                next_try_millis: NOW + 1,
            },
        );
        assert_eq!(
            acts(tend(vec![wanted("s")], &[], &backoff, NOW)),
            vec![("s".into(), Act::Wait)]
        );
        assert_eq!(
            acts(tend(vec![wanted("s")], &[], &backoff, NOW + 1)),
            vec![("s".into(), Act::Launch { clear: vec![] })]
        );
    }

    #[test]
    fn a_live_session_is_kept_even_while_its_seat_backs_off() {
        let mut backoff = BTreeMap::new();
        backoff.insert(
            key("s"),
            Backoff {
                failures: 3,
                next_try_millis: NOW + 60_000,
            },
        );
        let live = held("x1", "s", true, NOW - 1);
        assert_eq!(
            acts(tend(vec![wanted("s")], &[live], &backoff, NOW)),
            vec![("s".into(), Act::Keep { steady: false })]
        );
    }

    #[test]
    fn sessions_of_seats_not_wanted_are_left_alone() {
        // An orphan, or a seat of a fleet that is off: not wanted, not acted on.
        let orphan = held("x9", "gone", false, 0);
        assert_eq!(
            tend(vec![], std::slice::from_ref(&orphan), &BTreeMap::new(), NOW),
            vec![]
        );
        // A wanted seat never clears another seat's session.
        assert_eq!(
            acts(tend(vec![wanted("s")], &[orphan], &BTreeMap::new(), NOW)),
            vec![("s".into(), Act::Launch { clear: vec![] })]
        );
    }

    #[test]
    fn the_backoff_waits_nothing_then_five_seconds_doubling_to_five_minutes() {
        let waits: Vec<i64> = (0..10)
            .scan(None, |prev: &mut Option<Backoff>, _| {
                let next = Backoff::after_failure(prev.as_ref(), NOW);
                *prev = Some(next);
                Some(next.next_try_millis - NOW)
            })
            .collect();
        assert_eq!(
            waits,
            vec![
                0, 5_000, 10_000, 20_000, 40_000, 80_000, 160_000, 300_000, 300_000, 300_000
            ]
        );
        let huge = Backoff {
            failures: u32::MAX,
            next_try_millis: 0,
        };
        assert_eq!(
            Backoff::after_failure(Some(&huge), NOW).next_try_millis,
            NOW + 300_000
        );
    }

    #[test]
    fn a_session_ending_before_ten_minutes_ended_early() {
        assert!(ended_early(NOW - 1_000, NOW));
        assert!(!ended_early(NOW - STEADY_MILLIS, NOW));
    }
}
