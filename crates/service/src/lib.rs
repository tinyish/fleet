//! The service-manager driver: runs the supervisor under launchd on macOS
//! and systemd user units on Linux. The only crate that starts programs.

mod launchd;
pub mod render;
mod systemd;

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use fleet_core::paths::Os;
use fleet_core::tools::{FileStep, ServiceManager, ToolError};

/// The supervisor's launchd label.
pub const LAUNCHD_LABEL: &str = "com.tinyish.fleet.supervisor";

/// The supervisor's systemd user unit.
pub const SYSTEMD_UNIT: &str = "fleet-supervisor.service";

/// The driver for this machine's service manager, if tinyfleet runs here.
#[must_use]
pub fn for_this_os() -> Option<Box<dyn ServiceManager>> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    match Os::this()? {
        Os::MacOs => Some(Box::new(launchd::Launchd::new(var("HOME")?.into()))),
        Os::Linux => {
            let config = var("XDG_CONFIG_HOME")
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| var("HOME").map(|h| std::path::PathBuf::from(h).join(".config")))?;
            Some(Box::new(systemd::Systemd::new(config.join("systemd/user"))))
        }
    }
}

/// Runs one outside program and returns its output, or what went wrong
/// when it fails to start or exits non-zero.
fn run(program: &str, args: &[&OsStr]) -> Result<Output, ToolError> {
    let out =
        Command::new(program)
            .args(args)
            .output()
            .map_err(|error| ToolError::CouldNotRun {
                program: program.to_owned(),
                error,
            })?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(ToolError::Failed {
            command: format!("{program} {}", describe(args)),
            said: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        })
    }
}

/// A file step that failed, as a `ToolError`.
fn file(doing: FileStep, path: &Path, error: std::io::Error) -> ToolError {
    ToolError::File {
        doing,
        path: path.to_path_buf(),
        error,
    }
}

/// Refuses when an installed unit serves another state directory than
/// `ours`, so a run with `FLEET_STATE_DIR` set never takes over or removes
/// the supervisor of another one.
fn check_owner(installed: Option<PathBuf>, ours: &Path) -> Result<(), ToolError> {
    match installed {
        Some(theirs) if theirs != ours => Err(ToolError::OwnedElsewhere {
            theirs,
            ours: ours.to_path_buf(),
        }),
        _ => Ok(()),
    }
}

fn describe(args: &[&OsStr]) -> String {
    args.iter()
        .map(|a| a.to_string_lossy())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_serving_another_state_dir_is_refused() {
        let ours = Path::new("/tmp/ours");
        assert!(check_owner(None, ours).is_ok());
        assert!(check_owner(Some(PathBuf::from("/tmp/ours")), ours).is_ok());
        let err = check_owner(Some(PathBuf::from("/real/fleet")), ours).err();
        assert!(
            matches!(
                &err,
                Some(ToolError::OwnedElsewhere { theirs, ours: o })
                    if theirs == Path::new("/real/fleet") && o == ours
            ),
            "{err:?}"
        );
    }
}
