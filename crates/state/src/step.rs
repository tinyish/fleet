//! The steps the record holds, and how one is stored as a row: its name in
//! AIP-126 spelling in `step`, and its fields as a JSON object in `detail`.

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
                why: Status::Missing {
                    path: "/f/fleet.toml".into(),
                },
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
                "FLEET_CONFIG_BROKEN"
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
}
