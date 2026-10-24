//! The git driver: runs the person's `git`. A driver, so it may start
//! programs; it depends on nothing in the workspace but `fleet-core`.

use std::process::Command;

use fleet_core::tools::{self, ToolError};

/// The `git` on the `PATH`.
#[derive(Debug, Default, Clone, Copy)]
pub struct Git;

impl tools::Git for Git {
    fn version(&self) -> Result<String, ToolError> {
        let out = Command::new("git")
            .arg("--version")
            .output()
            .map_err(|error| ToolError::CouldNotRun {
                program: "git".into(),
                error,
            })?;
        if !out.status.success() {
            return Err(ToolError::Failed {
                command: "git --version".into(),
                said: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            });
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}
