//! Fleets, their projects, roles and seats as state keeps them, and the
//! supervisor's own steps. Each write records its own steps, and drops the
//! messages its change strands.

use std::io;
use std::path::{Path, PathBuf};

use fleet_core::fleets::{Change, Fleet};
use fleet_core::protocol::response::ConfigFate;
use fleet_core::record::Step;
use fleet_core::seats::{self, Roster, RosterChange, SeatKey, Wanted};
use fleet_core::status::Status;
use rusqlite::{Connection, OptionalExtension, params};

use crate::{State, StateError, Write, now_millis, roster_in, shown, utf8};

impl State {
    /// Every seat wanted now, with its role, by fleet and seat.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn wanted_seats(&self) -> Result<Vec<Wanted>, StateError> {
        wanted_in(&self.conn)
    }
}

/// Every wanted seat, with its role, read through `conn`: a seat is wanted
/// when its fleet has a project on, and it is in its fleet's roster with a
/// role that exists. The one rule, for tending and for every check.
fn wanted_in(conn: &Connection) -> Result<Vec<Wanted>, StateError> {
    let on: Vec<String> = {
        let mut query =
            conn.prepare("SELECT DISTINCT fleet FROM projects WHERE active = 1 ORDER BY fleet")?;
        let rows = query.query_map([], |row| row.get(0))?;
        rows.collect::<Result<_, _>>()?
    };
    let mut wanted = Vec::new();
    for fleet in on.into_iter().map(PathBuf::from) {
        let roster = roster_in(conn, &fleet)?;
        wanted.extend(roster.seats.iter().filter_map(|seat| {
            roster.role(&seat.role).map(|role| Wanted {
                key: SeatKey {
                    fleet: fleet.clone(),
                    seat: seat.name.clone(),
                },
                role: role.clone(),
            })
        }));
    }
    Ok(wanted)
}

impl Write<'_> {
    /// Records that a supervisor took the lock and began answering.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn supervisor_started(&mut self, version: &str, pid: u32) -> Result<(), StateError> {
        self.record(&Step::SupervisorStarted {
            version: version.to_owned(),
            pid,
        })
    }

    /// Registers `fleet`, its projects on, runs `write_config`, and keeps
    /// `roster` as its roles and seats; then records it created, `adopted`
    /// when made from a `fleet.toml` already there. If writing the config
    /// fails, the transaction fails with it.
    ///
    /// # Errors
    ///
    /// When SQLite fails, a path is not UTF-8, or `write_config` fails.
    pub fn create_fleet(
        &mut self,
        fleet: &Fleet,
        roster: &Roster,
        adopted: bool,
        write_config: impl FnOnce() -> io::Result<()>,
    ) -> Result<(), StateError> {
        self.tx.execute(
            "INSERT INTO fleets (directory, create_time_millis) VALUES (?1, ?2)",
            params![utf8(&fleet.directory)?, now_millis()],
        )?;
        for project in &fleet.projects {
            self.tx.execute(
                "INSERT INTO projects (directory, fleet, active) VALUES (?1, ?2, 1)",
                params![utf8(project)?, utf8(&fleet.directory)?],
            )?;
        }
        write_config().map_err(StateError::Config)?;
        self.set_roster(&fleet.directory, roster)?;
        self.record(&Step::FleetCreated {
            directory: shown(&fleet.directory),
            projects: fleet.projects.iter().map(|p| shown(p)).collect(),
            adopted,
        })
    }

    /// Whether `key`'s seat is wanted as this transaction sees it, by the
    /// rule of [`State::wanted_seats`]; with `role_digest`, only while its
    /// role's digest is that one.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn seat_wanted(
        &self,
        key: &SeatKey,
        role_digest: Option<&str>,
    ) -> Result<bool, StateError> {
        Ok(wanted_in(&self.tx)?
            .iter()
            .any(|w| w.key == *key && role_digest.is_none_or(|digest| w.role.digest() == digest)))
    }

    /// Replaces the roles and seats of the fleet in `fleet` with `roster`'s,
    /// and records what moved: a role before the seats that name it, and
    /// after the seats that leave it. Then drops the messages held for each
    /// seat that left, recording each dropped.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a path is not UTF-8.
    pub fn set_roster(&mut self, fleet: &Path, roster: &Roster) -> Result<(), StateError> {
        let moved = seats::diff(&roster_in(&self.tx, fleet)?, roster);
        let directory = utf8(fleet)?;
        self.tx
            .execute("DELETE FROM roles WHERE fleet = ?1", params![directory])?;
        self.tx
            .execute("DELETE FROM seats WHERE fleet = ?1", params![directory])?;
        for role in &roster.roles {
            self.tx.execute(
                "INSERT INTO roles (fleet, label, adapter, model, prompt, directory)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    directory,
                    role.label,
                    role.adapter,
                    role.model,
                    role.prompt,
                    utf8(&role.directory)?
                ],
            )?;
        }
        for seat in &roster.seats {
            self.tx.execute(
                "INSERT INTO seats (fleet, name, role) VALUES (?1, ?2, ?3)",
                params![directory, seat.name, seat.role],
            )?;
        }
        roster_steps(&shown(fleet), &moved)
            .iter()
            .try_for_each(|step| self.record(step))?;
        moved
            .removed
            .iter()
            .try_for_each(|seat| self.drop_for_seat(directory, seat))
    }

    /// Removes the fleet in `directory`, and records it removed with the
    /// projects it served and what became of its config file. Its
    /// projects, roles and seats go with it, its seats' sessions stay, and
    /// the messages held for its seats are dropped, each recorded.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn remove_fleet(&mut self, directory: &Path, config: ConfigFate) -> Result<(), StateError> {
        let projects = {
            let mut query = self
                .tx
                .prepare("SELECT directory FROM projects WHERE fleet = ?1 ORDER BY directory")?;
            let rows = query.query_map(params![utf8(directory)?], |row| row.get(0))?;
            rows.collect::<Result<_, _>>()?
        };
        let removed = Step::FleetRemoved {
            directory: shown(directory),
            projects,
            config,
        };
        self.delete_fleet(directory, &removed)
    }

    /// Forgets the fleet in `directory`, whose directory no longer exists,
    /// as [`Write::remove_fleet`] removes one, and records it forgotten.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn forget_fleet(&mut self, directory: &Path) -> Result<(), StateError> {
        let forgotten = Step::FleetForgotten {
            directory: shown(directory),
        };
        self.delete_fleet(directory, &forgotten)
    }

    /// Deletes the fleet in `directory`, records `step`, and drops the
    /// messages held for its seats; nothing, for no such fleet.
    fn delete_fleet(&mut self, directory: &Path, step: &Step) -> Result<(), StateError> {
        let directory = utf8(directory)?;
        let deleted = self.tx.execute(
            "DELETE FROM fleets WHERE directory = ?1",
            params![directory],
        )?;
        if deleted == 0 {
            return Ok(());
        }
        self.record(step)?;
        self.drop_for_fleet(directory)
    }

    /// Takes `projects` out of the fleet in `fleet`, which serves them, and
    /// records them taken.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a path is not UTF-8.
    pub fn remove_projects(
        &mut self,
        fleet: &Path,
        projects: &[PathBuf],
    ) -> Result<(), StateError> {
        let mut taken = 0;
        for project in projects {
            taken += self.tx.execute(
                "DELETE FROM projects WHERE directory = ?1",
                params![utf8(project)?],
            )?;
        }
        if taken == 0 {
            return Ok(());
        }
        self.record(&Step::ProjectsTaken {
            fleet: shown(fleet),
            projects: projects.iter().map(|p| shown(p)).collect(),
        })
    }

    /// Turns one project on or off, and records it, with its fleet.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the path is not UTF-8.
    pub fn set_active(&mut self, project: &Path, active: bool) -> Result<(), StateError> {
        let fleet: Option<String> = self
            .tx
            .query_row(
                "SELECT fleet FROM projects WHERE directory = ?1",
                params![utf8(project)?],
                |row| row.get(0),
            )
            .optional()?;
        let Some(fleet) = fleet else {
            return Ok(());
        };
        self.tx.execute(
            "UPDATE projects SET active = ?1 WHERE directory = ?2",
            params![active, utf8(project)?],
        )?;
        let project = shown(project);
        self.record(&if active {
            Step::ProjectOn { project, fleet }
        } else {
            Step::ProjectOff { project, fleet }
        })
    }

    /// Turns every project of each fleet in `on` on, and of each in `off`
    /// off, and records every fleet on.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a path is not UTF-8.
    pub fn all_on(&mut self, on: &[PathBuf], off: &[PathBuf]) -> Result<(), StateError> {
        let listed = on
            .iter()
            .map(|f| (f, true))
            .chain(off.iter().map(|f| (f, false)));
        for (fleet, active) in listed {
            self.tx.execute(
                "UPDATE projects SET active = ?1 WHERE fleet = ?2",
                params![active, utf8(fleet)?],
            )?;
        }
        self.record(&Step::EveryFleetOn)
    }

    /// Turns every project off, and records it.
    ///
    /// # Errors
    ///
    /// When SQLite fails.
    pub fn all_off(&mut self) -> Result<(), StateError> {
        self.tx.execute("UPDATE projects SET active = 0", [])?;
        self.record(&Step::EveryFleetOff)
    }

    /// Applies what re-reading every fleet's config changed, and records
    /// each fleet's projects removed, then added. A project removed from one
    /// fleet and added to another moves, keeping its on or off; a project
    /// only added comes in off, and one only removed is dropped.
    ///
    /// # Errors
    ///
    /// When SQLite fails or a path is not UTF-8.
    pub fn apply_reread(&mut self, changes: &[(PathBuf, Change)]) -> Result<(), StateError> {
        let added: Vec<&PathBuf> = changes.iter().flat_map(|(_, c)| &c.added).collect();
        for (_, change) in changes {
            for project in change.removed.iter().filter(|p| !added.contains(p)) {
                self.tx.execute(
                    "DELETE FROM projects WHERE directory = ?1",
                    params![utf8(project)?],
                )?;
            }
        }
        for (fleet, change) in changes {
            for project in &change.added {
                self.tx.execute(
                    "INSERT INTO projects (directory, fleet, active) VALUES (?1, ?2, 0)
                     ON CONFLICT (directory) DO UPDATE SET fleet = excluded.fleet",
                    params![utf8(project)?, utf8(fleet)?],
                )?;
            }
        }
        for (fleet, change) in changes {
            let fleet = shown(fleet);
            let removed = change.removed.iter().map(|p| Step::ProjectRemoved {
                fleet: fleet.clone(),
                project: shown(p),
            });
            let added = change.added.iter().map(|p| Step::ProjectAdded {
                fleet: fleet.clone(),
                project: shown(p),
            });
            removed
                .chain(added)
                .try_for_each(|step| self.record(&step))?;
        }
        Ok(())
    }

    /// Records that the config of the fleet in `fleet` did not read, or
    /// claimed another fleet's project, for `why`.
    ///
    /// # Errors
    ///
    /// When SQLite fails or the step cannot be stored.
    pub fn config_broken(&mut self, fleet: &Path, why: &Status) -> Result<(), StateError> {
        self.record(&Step::FleetConfigBroken {
            fleet: shown(fleet),
            why: why.clone(),
        })
    }
}

/// The steps that record `moved` in the fleet `fleet`, in their order.
fn roster_steps(fleet: &str, moved: &RosterChange) -> Vec<Step> {
    let f = || fleet.to_owned();
    let roles_added = moved.roles_added.iter().map(|role| Step::RoleAdded {
        fleet: f(),
        role: role.clone(),
    });
    let changed = moved.changed.iter().map(|role| Step::RoleChanged {
        fleet: f(),
        role: role.clone(),
    });
    let added = moved.added.iter().map(|seat| Step::SeatAdded {
        fleet: f(),
        seat: seat.name.clone(),
        role: seat.role.clone(),
    });
    let seats_moved = moved.moved.iter().map(|seat| Step::SeatChanged {
        fleet: f(),
        seat: seat.name.clone(),
        role: seat.role.clone(),
    });
    let removed = moved.removed.iter().map(|seat| Step::SeatRemoved {
        fleet: f(),
        seat: seat.clone(),
    });
    let roles_removed = moved.roles_removed.iter().map(|role| Step::RoleRemoved {
        fleet: f(),
        role: role.clone(),
    });
    roles_added
        .chain(changed)
        .chain(added)
        .chain(seats_moved)
        .chain(removed)
        .chain(roles_removed)
        .collect()
}
