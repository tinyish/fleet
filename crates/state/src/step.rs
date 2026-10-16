//! The steps the record holds, and how one is stored as a row: its name in
//! AIP-126 spelling in `step`, and its fields as a JSON object in `detail`.

use serde::{Deserialize, Serialize};

/// One step tinyfleet records. Its variant name, in AIP-126 spelling, is the
/// row's `step`; its fields are the row's `detail`. No variant may have a
/// field named `step`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "step", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Step {
    /// A supervisor took the lock and began answering.
    SupervisorStarted {
        /// The supervisor's version.
        version: String,
        /// Its process id.
        pid: u32,
    },
    /// Every fleet was turned on.
    EveryFleetOn,
    /// Every fleet was turned off.
    EveryFleetOff,
    /// A fleet was created.
    FleetCreated {
        /// Its directory.
        directory: String,
        /// The projects it serves.
        projects: Vec<String>,
    },
}

/// One row of the record, as read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The row's id; it orders the record.
    pub id: i64,
    /// When the step happened, in milliseconds since the Unix epoch.
    pub occur_time_millis: i64,
    /// What happened.
    pub step: Step,
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

    /// The step a row holds.
    ///
    /// # Errors
    ///
    /// When `detail` is not a JSON object, the name is not a known step, or
    /// the fields do not match it.
    pub fn from_row(name: &str, detail: &str) -> Result<Step, serde_json::Error> {
        let mut value: serde_json::Value = serde_json::from_str(detail)?;
        if let Some(fields) = value.as_object_mut() {
            fields.insert("step".to_owned(), name.into());
        }
        serde_json::from_value(value)
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
        ]
    }

    #[test]
    fn every_step_round_trips_through_a_row() {
        for step in all() {
            let (name, detail) = step.to_row().unwrap_or_default();
            assert_eq!(Step::from_row(&name, &detail).ok(), Some(step));
        }
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
                "FLEET_CREATED"
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
    fn an_unknown_step_or_a_broken_detail_is_an_error() {
        assert!(Step::from_row("NO_SUCH_STEP", "{}").is_err());
        assert!(Step::from_row("SUPERVISOR_STARTED", "{}").is_err());
        assert!(Step::from_row("EVERY_FLEET_ON", "not json").is_err());
    }
}
