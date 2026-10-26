//! The steps the record holds, and how one is stored as a row: its name in
//! AIP-126 spelling in `step`, and its fields as a JSON object in `detail`.

use fleet_core::protocol::response::ConfigFate;
use fleet_core::status::Status;
use serde::Serialize;

/// One step tinyfleet records. Its variant name, in AIP-126 spelling, is the
/// row's `step`; its fields are the row's `detail`. No variant may have a
/// field named `step`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "step", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Step {
    /// A supervisor took the lock and began answering.
    SupervisorStarted {
        /// The supervisor's version.
        version: String,
        /// Its process id.
        pid: u32,
    },
    /// Every project of every fleet whose config read was turned on.
    EveryFleetOn,
    /// Every project was turned off.
    EveryFleetOff,
    /// A fleet was created.
    FleetCreated {
        /// Its directory.
        directory: String,
        /// The projects it serves.
        projects: Vec<String>,
        /// Whether it was made from a `fleet.toml` already there.
        adopted: bool,
    },
    /// One project was turned on.
    ProjectOn {
        /// The project.
        project: String,
    },
    /// One project was turned off.
    ProjectOff {
        /// The project.
        project: String,
    },
    /// Re-reading a fleet's config added a project to it.
    ProjectAdded {
        /// The fleet's directory.
        fleet: String,
        /// The project.
        project: String,
    },
    /// Re-reading a fleet's config removed a project from it.
    ProjectRemoved {
        /// The fleet's directory.
        fleet: String,
        /// The project.
        project: String,
    },
    /// A fleet's config did not read, or claimed another fleet's project, so
    /// nothing of it was turned on.
    FleetConfigBroken {
        /// The fleet's directory.
        fleet: String,
        /// Why, as the facts the CLI words.
        why: Status,
    },
    /// A fleet was removed, and with it its projects.
    FleetRemoved {
        /// The fleet's directory.
        directory: String,
        /// The projects it served.
        projects: Vec<String>,
        /// What became of its config file.
        config: ConfigFate,
    },
    /// Projects were taken out of a wingman fleet, which stays.
    ProjectsTaken {
        /// The fleet's directory.
        fleet: String,
        /// The projects taken out.
        projects: Vec<String>,
    },
    /// A fleet whose directory no longer exists was forgotten by a start.
    FleetForgotten {
        /// The fleet's directory.
        directory: String,
    },
    /// A session was launched: its agent started in tmux.
    SessionLaunched {
        /// tinyfleet's id for it.
        session: String,
        /// The adapter that said what to run, as it was named: a path, or
        /// `<name>@<version>`.
        adapter: String,
        /// The program `adapter` resolved to, as the session's row keeps it.
        program: String,
        /// Where its agent works.
        directory: String,
        /// The model it was asked to run.
        #[serde(skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// What the agent was first told.
        #[serde(skip_serializing_if = "Option::is_none")]
        prompt: Option<String>,
        /// The fleet of the seat it was launched for.
        #[serde(skip_serializing_if = "Option::is_none")]
        fleet: Option<String>,
        /// That seat.
        #[serde(skip_serializing_if = "Option::is_none")]
        seat: Option<String>,
    },
    /// A session was stopped: its tmux session ended and it is forgotten.
    SessionStopped {
        /// tinyfleet's id for it.
        session: String,
    },
    /// Re-reading a fleet's config gave it a seat.
    SeatAdded {
        /// The fleet's directory.
        fleet: String,
        /// The seat.
        seat: String,
        /// The label of its role.
        role: String,
    },
    /// Re-reading a fleet's config took a seat away; its session, if any,
    /// is left running, never revived.
    SeatRemoved {
        /// The fleet's directory.
        fleet: String,
        /// The seat.
        seat: String,
    },
    /// Re-reading a fleet's config changed how a role runs; its seats'
    /// sessions carry on as launched until their next revival.
    RoleChanged {
        /// The fleet's directory.
        fleet: String,
        /// The role's label.
        role: String,
    },
    /// Re-reading a fleet's config moved a seat to another role; its session
    /// carries on as launched until its next revival.
    SeatChanged {
        /// The fleet's directory.
        fleet: String,
        /// The seat.
        seat: String,
        /// The label of the role it now runs as.
        role: String,
    },
    /// Re-reading a fleet's config gave it a role.
    RoleAdded {
        /// The fleet's directory.
        fleet: String,
        /// The role's label.
        role: String,
    },
    /// Re-reading a fleet's config took a role away.
    RoleRemoved {
        /// The fleet's directory.
        fleet: String,
        /// The role's label.
        role: String,
    },
    /// A seat's session whose pane had ended was cleared, before it was
    /// launched again.
    SessionGone {
        /// tinyfleet's id for it.
        session: String,
        /// The fleet's directory.
        fleet: String,
        /// The seat.
        seat: String,
    },
    /// Launching a seat failed; it backs off.
    SeatLaunchFailed {
        /// The fleet's directory.
        fleet: String,
        /// The seat.
        seat: String,
        /// Why, as the facts the CLI words.
        why: Status,
    },
}

impl Step {
    /// The step as a row: its name and its fields as a JSON object.
    ///
    /// # Errors
    ///
    /// When the step cannot be turned into JSON.
    pub fn to_row(&self) -> Result<(String, String), serde_json::Error> {
        let mut value = serde_json::to_value(self)?;
        let name = value
            .as_object_mut()
            .and_then(|fields| fields.remove("step"))
            .and_then(|name| name.as_str().map(str::to_owned))
            .unwrap_or_default();
        Ok((name, value.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_core::status;

    fn all() -> Vec<Step> {
        vec![
            Step::SupervisorStarted {
                version: "0.0.0".into(),
                pid: 42,
            },
            Step::EveryFleetOn,
            Step::EveryFleetOff,
            Step::FleetCreated {
                directory: "/f/work".into(),
                projects: vec!["/c/app".into()],
                adopted: false,
            },
            Step::ProjectOn {
                project: "/c/app".into(),
            },
            Step::ProjectOff {
                project: "/c/app".into(),
            },
            Step::ProjectAdded {
                fleet: "/f".into(),
                project: "/c/a".into(),
            },
            Step::ProjectRemoved {
                fleet: "/f".into(),
                project: "/c/a".into(),
            },
            Step::FleetConfigBroken {
                fleet: "/f".into(),
                why: Status::File(status::File::Missing {
                    path: "/f/fleet.toml".into(),
                }),
            },
            Step::FleetRemoved {
                directory: "/f".into(),
                projects: vec!["/c/a".into()],
                config: ConfigFate::Kept,
            },
            Step::ProjectsTaken {
                fleet: "/f".into(),
                projects: vec!["/c/a".into()],
            },
            Step::FleetForgotten {
                directory: "/f".into(),
            },
            Step::SessionLaunched {
                session: "s1".into(),
                adapter: "claude-code@2.1.285".into(),
                program: "/s/adapters/claude-code/2.1.285/fleet-claude-code".into(),
                directory: "/c/app".into(),
                model: Some("sonnet".into()),
                prompt: None,
                fleet: Some("/c/app".into()),
                seat: Some("scribe".into()),
            },
            Step::SessionStopped {
                session: "s1".into(),
            },
            Step::SeatAdded {
                fleet: "/c/app".into(),
                seat: "scribe".into(),
                role: "writer".into(),
            },
            Step::SeatRemoved {
                fleet: "/c/app".into(),
                seat: "scribe".into(),
            },
            Step::RoleChanged {
                fleet: "/c/app".into(),
                role: "writer".into(),
            },
            Step::SeatChanged {
                fleet: "/c/app".into(),
                seat: "scribe".into(),
                role: "critic".into(),
            },
            Step::RoleAdded {
                fleet: "/c/app".into(),
                role: "critic".into(),
            },
            Step::RoleRemoved {
                fleet: "/c/app".into(),
                role: "writer".into(),
            },
            Step::SessionGone {
                session: "s1".into(),
                fleet: "/c/app".into(),
                seat: "scribe".into(),
            },
            Step::SeatLaunchFailed {
                fleet: "/c/app".into(),
                seat: "scribe".into(),
                why: Status::File(status::File::Missing {
                    path: "/c/app/notes".into(),
                }),
            },
        ]
    }

    #[test]
    fn names_use_aip_126_spelling() {
        let names: Vec<String> = all()
            .iter()
            .filter_map(|s| s.to_row().ok())
            .map(|r| r.0)
            .collect();
        assert_eq!(
            names,
            [
                "SUPERVISOR_STARTED",
                "EVERY_FLEET_ON",
                "EVERY_FLEET_OFF",
                "FLEET_CREATED",
                "PROJECT_ON",
                "PROJECT_OFF",
                "PROJECT_ADDED",
                "PROJECT_REMOVED",
                "FLEET_CONFIG_BROKEN",
                "FLEET_REMOVED",
                "PROJECTS_TAKEN",
                "FLEET_FORGOTTEN",
                "SESSION_LAUNCHED",
                "SESSION_STOPPED",
                "SEAT_ADDED",
                "SEAT_REMOVED",
                "ROLE_CHANGED",
                "SEAT_CHANGED",
                "ROLE_ADDED",
                "ROLE_REMOVED",
                "SESSION_GONE",
                "SEAT_LAUNCH_FAILED"
            ]
        );
    }

    #[test]
    fn detail_holds_the_fields_and_is_an_empty_object_without_them() {
        let started = Step::SupervisorStarted {
            version: "0.0.0".into(),
            pid: 42,
        };
        assert_eq!(
            started.to_row().ok().map(|r| r.1),
            Some(r#"{"pid":42,"version":"0.0.0"}"#.to_owned())
        );
        assert_eq!(
            Step::EveryFleetOn.to_row().ok().map(|r| r.1),
            Some("{}".to_owned())
        );
    }

    #[test]
    fn a_debug_sessions_launch_names_no_seat() {
        let launched = Step::SessionLaunched {
            session: "s1".into(),
            adapter: "/a/claude".into(),
            program: "/a/claude".into(),
            directory: "/c/app".into(),
            model: None,
            prompt: None,
            fleet: None,
            seat: None,
        };
        assert_eq!(
            launched.to_row().ok().map(|r| r.1),
            Some(
                r#"{"adapter":"/a/claude","directory":"/c/app","program":"/a/claude","session":"s1"}"#
                    .to_owned()
            )
        );
    }
}
