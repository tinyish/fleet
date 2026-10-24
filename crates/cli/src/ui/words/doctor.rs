//! Every sentence of `fleet doctor`'s report: what each check is called,
//! each verdict, and each finding with what to do about it.

use std::io::ErrorKind;

use fleet_core::paths::PathError;
use fleet_core::tools::ToolError;

use super::{path_error, refused, socket, tool};
use crate::cmd::doctor::{Checked, Found, Verdict};
use crate::ui::output::Ink;

/// What a check is called in the report.
pub(crate) fn checked(checked: Checked) -> &'static str {
    match checked {
        Checked::ServiceManager => "service manager",
        Checked::Git => "git",
        Checked::Supervisor => "supervisor",
        Checked::State => "state",
        Checked::Project => "project",
    }
}

/// A verdict, as a script reads it.
pub(crate) fn verdict(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Fine => "fine",
        Verdict::Warn => "warning",
        Verdict::Missing => "missing",
        Verdict::CouldNotTell => "could not tell",
    }
}

/// What a fine check says before what was found, on a terminal.
pub(crate) const HEALTHY: &str = "healthy";

/// What doctor found, in words.
pub(crate) fn found(found: &Found, ink: Ink) -> String {
    match found {
        Found::ManagerAnswers { name } => format!("{name} is up and running"),
        Found::ManagerSilent { name, error } => {
            format!("{name} does not answer: {}", tool(error, ink))
        }
        Found::NoManager => format!("cannot be found: {} is not set", ink.accent("HOME")),
        Found::GitVersion(version) => version.clone(),
        Found::GitBroken(ToolError::CouldNotRun { error, .. })
            if error.kind() == ErrorKind::NotFound =>
        {
            format!("{} is not on the PATH", ink.accent("git"))
        }
        Found::GitBroken(error) => tool(error, ink),
        Found::Running { version } => format!("running, version {version}"),
        Found::OtherVersion { running, ours } => {
            format!("running version {running}, but this fleet binary is {ours}")
        }
        Found::NotRunning { .. } => "not running".into(),
        Found::InstalledButSilent { .. } => {
            "its unit is installed but it does not answer, so it may be failing to start".into()
        }
        Found::SupervisorSlow { waited, .. } => {
            format!("did not answer within {}s", waited.as_secs())
        }
        Found::SupervisorFailed(error) => format!("did not answer: {}", socket(error)),
        Found::OddHello => "answered something other than hello".into(),
        Found::NowhereToLook => "nowhere to look without a state directory".into(),
        Found::SocketTooLong(path) => format!(
            "the socket path {} is longer than {} bytes",
            ink.path(path),
            fleet_core::protocol::MAX_SOCKET_PATH
        ),
        Found::StateOpen { dir, schema } => {
            format!("{}: open at schema {schema}", ink.path(dir))
        }
        Found::StateUnsaid => {
            "the running supervisor is too old to say how the state stands".into()
        }
        Found::StateUnknown => "the supervisor did not say how the state stands".into(),
        Found::NoStateDir(error) => path_error(error, ink),
        Found::FleetPresent { dir, projects } => match projects {
            1 => format!("{}: 1 project, all present", ink.path(dir)),
            n => format!("{}: {n} projects, all present", ink.path(dir)),
        },
        Found::FleetBroken { why, .. } => refused(why, ink),
        Found::NoCurrentDir(error) => format!("cannot read the current directory: {error}"),
    }
}

/// What to do about a finding, when there is something to do.
pub(crate) fn found_next(found: &Found, ink: Ink) -> Option<String> {
    let start = ink.accent("fleet start");
    Some(match found {
        Found::NoManager | Found::NoStateDir(PathError::NoHome) => {
            format!("set {}", ink.accent("HOME"))
        }
        Found::NoStateDir(PathError::NotAbsolute { var, .. }) => {
            format!("set {} to an absolute path", ink.accent(var))
        }
        Found::SocketTooLong(_) => format!(
            "set {} to a shorter directory",
            ink.accent("FLEET_STATE_DIR")
        ),
        Found::GitBroken(ToolError::CouldNotRun { error, .. })
            if error.kind() == ErrorKind::NotFound =>
        {
            format!("install {} and put it on your PATH", ink.accent("git"))
        }
        Found::OtherVersion { .. } => {
            format!("run {}, then {start}", ink.accent("fleet stop"))
        }
        Found::NotRunning { unit_serves: None } => format!("run {start} to start it"),
        Found::NotRunning {
            unit_serves: Some(theirs),
        } => format!(
            "{start} here is refused while the installed unit serves {}",
            ink.path(theirs)
        ),
        Found::InstalledButSilent { log } | Found::SupervisorSlow { log: Some(log), .. } => {
            format!("check its log: {}", ink.accent(log))
        }
        Found::FleetBroken { dir, .. } => {
            format!("fix the {} in {}", ink.accent("fleet.toml"), ink.path(dir))
        }
        _ => return None,
    })
}
