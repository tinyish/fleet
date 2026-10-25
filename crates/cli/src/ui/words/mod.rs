//! Every sentence tinyfleet says to a person. The supervisor replies with
//! facts, `protocol::Response` or `protocol::Status`; a command ends with the
//! CLI's own `Response` or `Status`. This file turns all of them into words
//! (core.md invariant 9), colouring the variable parts with the ink of the
//! stream they are written to.

use fleet_core::protocol;
use fleet_core::protocol::ProtocolError;
use fleet_core::tools::{BadUnitPath, FileStep, ToolError, UnitPath};

use crate::protocol::{Doing, Doubt, During, Problem, Response, Startup, Status};
use crate::ui::output::Ink;

mod adapters;
mod create;
mod doctor;
pub(crate) mod remove;
mod session;
pub(crate) mod uninstall;
use create::created;
pub(crate) use doctor::{HEALTHY, checked, found, found_next, verdict};

/// What `fleet --help` says tinyfleet is.
pub(crate) const ABOUT: &str = "tinyfleet runs AI coding agents on your projects";

/// The help for `--non-interactive`.
pub(crate) const NON_INTERACTIVE_HELP: &str =
    "Never ask; wherever fleet would ask, take the answer that keeps going";

/// The help for `fleet create`.
pub(crate) const CREATE_ABOUT: &str =
    "Create a fleet here: this repository, or a wingman fleet serving --project repositories";

/// The help for `--project`.
pub(crate) const PROJECT_HELP: &str = "A repository the fleet serves; repeat for more. \
     Without it, this directory is the one project";

/// The help for `fleet doctor`.
pub(crate) const DOCTOR_ABOUT: &str =
    "Check what tinyfleet runs on: each item fine, missing or could not tell; changes nothing";

/// The help for `fleet start`.
pub(crate) const START_ABOUT: &str = "Start the supervisor, re-read every fleet's config and \
     turn every project on; it comes back after a reboot until stop";

/// The help for `fleet start --project`.
pub(crate) const START_PROJECT_HELP: &str = "Turn only this project on, by its repository's root";

/// The help for `fleet stop`.
pub(crate) const STOP_ABOUT: &str =
    "Turn every project off and stop the supervisor, so it does not come back";

/// The help for `fleet stop --project`.
pub(crate) const STOP_PROJECT_HELP: &str =
    "Turn only this project off, leaving the supervisor and the rest of its fleet as they are";

/// The question asked at a terminal when the supervisor is not running.
pub(crate) const START_PROMPT: &str = "The fleet supervisor is not running. Start it?";

/// A yes-or-no prompt's two answers.
pub(crate) const YES: &str = "Yes";
/// See `YES`.
pub(crate) const NO: &str = "No";

/// What a cancelled prompt shows.
pub(crate) const CANCELLED: &str = "cancelled";

/// The spinner's words while the supervisor comes up.
pub(crate) const STARTING: &str = "starting the supervisor";

/// The spinner's words while the supervisor goes down.
pub(crate) const STOPPING: &str = "stopping the supervisor";

/// The spinner while `fleet stop` waits for a supervisor that is running but
/// not answering.
pub(crate) const WAITING: &str = "waiting for the supervisor to answer";

/// What a command did.
pub(crate) fn response(response: &Response, ink: Ink) -> String {
    match response {
        Response::Created {
            response,
            startup: Startup::Started,
        } => format!("{}; {}", started_supervisor(), created(response, ink)),
        Response::Created { response, .. } => created(response, ink),
        Response::Started { response, startup } => started(response, *startup, ink),
        Response::ProjectOn { response, startup } => project_on(response, *startup, ink),
        Response::ProjectOff(response) => project_off(response, ink),
        Response::SupervisorStopped => "the supervisor is stopped".into(),
        Response::SupervisorWasNotRunning => "the supervisor was not running".into(),
        Response::Cancelled => "nothing was removed".into(),
        Response::Uninstalled {
            binary,
            state_dir,
            kept_state_dir,
        } => uninstall::uninstalled(binary, state_dir, *kept_state_dir, ink),
        Response::Removed {
            response,
            startup: Startup::Started,
        } => format!(
            "{}; {}",
            started_supervisor(),
            remove::removed(response, ink)
        ),
        Response::Removed { response, .. } => remove::removed(response, ink),
    }
}

/// Why a command did not do what was asked.
pub(crate) fn status(status: &Status, ink: Ink) -> String {
    match status {
        Status::Refused(status) => refused(status, ink),
        Status::Cannot(problem) => cannot(problem, ink),
        Status::NotRunning => "the supervisor is not running".into(),
        Status::NeedsAnswer => uninstall::NEEDS_ANSWER.into(),
        Status::CouldNotTell(doubt) => unsure(doubt),
        // `output` prints these as a done line and one line per fleet that
        // stayed off; this is the same in one line.
        Status::PartlyOn { response, .. } => response
            .stayed_off
            .iter()
            .map(|off| stayed_off(off, ink))
            .collect::<Vec<_>>()
            .join("; "),
    }
}

/// The done line of a start that left some fleets off.
pub(crate) fn partly_on(
    response: &protocol::response::AllOn,
    startup: Startup,
    ink: Ink,
) -> String {
    let on = match response.on.as_slice() {
        [] => String::new(),
        [one] => format!("; {} is enabled", ink.path(one)),
        many => format!("; {} are enabled", ink.paths(many)),
    };
    let mut said = format!("{}{on}", supervisor_was(startup));
    if let Some(picked) = picked_up(&response.picked_up, ink) {
        said = format!("{said}; {picked}");
    }
    said
}

/// One fleet that stayed off, and why.
pub(crate) fn stayed_off(off: &protocol::response::StayedOff, ink: Ink) -> String {
    format!(
        "the fleet in {} stayed off: {}",
        ink.path(&off.fleet),
        refused(&off.why, ink)
    )
}

/// What to do next after a command that did what was asked, when there is
/// something to do.
pub(crate) fn next_after(response: &Response, ink: Ink) -> Option<String> {
    match response {
        Response::Started { response, .. } if response.fleets.is_empty() => Some(format!(
            "run {} in a repository to make one",
            ink.accent("fleet create")
        )),
        _ => None,
    }
}

/// What start did when every config read.
fn started(response: &protocol::response::AllOn, startup: Startup, ink: Ink) -> String {
    let mut said = supervisor_was(startup).to_owned();
    if response.fleets.is_empty() {
        return format!("{said}; there are no fleets yet");
    }
    if let Some(picked) = picked_up(&response.picked_up, ink) {
        said = format!("{said}; {picked}");
    }
    if startup == Startup::Started {
        said.push_str(" -- cleared for takeoff");
    }
    said
}

/// What start --project did.
fn project_on(response: &protocol::response::ProjectOn, startup: Startup, ink: Ink) -> String {
    let mut said = format!("{} is on", ink.path(&response.project));
    if startup == Startup::Started {
        said = format!("{}; {said}", started_supervisor());
    }
    if let Some(picked) = picked_up(&response.picked_up, ink) {
        said = format!("{said}; {picked}");
    }
    said
}

/// What stop --project did.
fn project_off(response: &protocol::response::ProjectOff, ink: Ink) -> String {
    let off = format!("{} is off", ink.path(&response.project));
    match response.others.as_slice() {
        [] => off,
        [one] => format!("{off}; {} keeps its own on or off", ink.path(one)),
        many => format!("{off}; {} keep their own on or off", ink.paths(many)),
    }
}

/// What re-reading the configs changed, if anything.
fn picked_up(picked: &[protocol::response::PickedUp], ink: Ink) -> Option<String> {
    let mut parts = Vec::new();
    for p in picked {
        if !p.added.is_empty() {
            parts.push(format!(
                "added {} to the fleet in {}",
                ink.paths(&p.added),
                ink.path(&p.fleet)
            ));
        }
        if !p.removed.is_empty() {
            parts.push(format!(
                "removed {} from the fleet in {}",
                ink.paths(&p.removed),
                ink.path(&p.fleet)
            ));
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(format!(
        "picked up from {}: {}",
        ink.accent("fleet.toml"),
        parts.join(", ")
    ))
}

/// What to do about a status, when there is something to do: the hint
/// line printed under it.
pub(crate) fn next(status: &Status, ink: Ink) -> Option<String> {
    let create = ink.accent("fleet create");
    let project_flag = ink.accent("--project");
    Some(match status {
        Status::NotRunning => format!("run {} to start it", ink.accent("fleet start")),
        Status::NeedsAnswer => uninstall::add_flag(ink),
        Status::Cannot(Problem::Uninstall { .. }) => uninstall::try_again(ink),
        Status::Refused(protocol::Status::DirectoryNotInRepo { .. }) => {
            format!("run {create} at a repository's root")
        }
        Status::Refused(protocol::Status::DirectoryNotRepoRoot { root, .. }) => {
            format!("run {create} in {}", ink.whole_path(root))
        }
        Status::Refused(protocol::Status::ProjectNotRepoRoot { root, .. }) => {
            format!(
                "pass the repository's root, {}, instead",
                ink.whole_path(root)
            )
        }
        Status::Refused(protocol::Status::ProjectIsFleetDirectory { .. }) => {
            format!("run {create} without {project_flag} to make a fleet of this repository itself")
        }
        Status::Cannot(Problem::SocketPathTooLong(_)) => format!(
            "set {} to a shorter directory",
            ink.accent("FLEET_STATE_DIR")
        ),
        Status::Cannot(Problem::ServiceManager {
            error: ToolError::OwnedElsewhere { theirs, .. },
            ..
        }) => format!(
            "run {} first to stop it",
            ink.accent(format!(
                "FLEET_STATE_DIR={} fleet stop",
                shell_quoted(&theirs.display().to_string())
            ))
        ),
        Status::Refused(protocol::Status::InsideProject { project, .. }) => {
            format!("name its root, {}", ink.whole_path(project))
        }
        Status::Refused(protocol::Status::ConfigDiffers { .. }) => create::take_it_up(ink),
        Status::Refused(protocol::Status::NotFleetDirectory { fleet, .. }) => format!(
            "run {} in {}",
            ink.accent("fleet remove"),
            ink.whole_path(fleet)
        ),
        Status::Refused(protocol::Status::CannotTurnOn { fleet, .. }) => format!(
            "fix the {} in {}, then try again",
            ink.accent("fleet.toml"),
            ink.path(fleet)
        ),
        Status::PartlyOn { .. } => format!(
            "fix each {} named above, then run {}",
            ink.accent("fleet.toml"),
            ink.accent("fleet start")
        ),
        Status::CouldNotTell(
            Doubt::NoAnswer { log, .. } | Doubt::StoppedUnanswered { log, .. },
        ) => {
            format!("check its log: {}", ink.accent(log))
        }
        _ => return None,
    })
}

/// `s` as one shell word: as it is when that is safe, else single-quoted.
fn shell_quoted(s: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "/._-+:@%".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// Why the supervisor said no.
fn refused(status: &protocol::Status, ink: Ink) -> String {
    match status {
        protocol::Status::DirectoryNotRepoRoot { path, root } => format!(
            "{} is inside the repository at {}",
            ink.path(path),
            ink.path(root)
        ),
        protocol::Status::DirectoryNotInRepo { path }
        | protocol::Status::ProjectNotInRepo { path } => {
            format!("{} is not in a git repository", ink.path(path))
        }
        protocol::Status::ProjectNotRepoRoot { path, .. } => {
            format!("{} is not a repository's root", ink.path(path))
        }
        protocol::Status::ProjectIsFleetDirectory { path } => {
            format!("{} is the fleet's own directory", ink.path(path))
        }
        protocol::Status::WingmanInsideProject { directory, project } => format!(
            "a wingman fleet's directory must be outside the repositories it serves; \
             {} is inside {}",
            ink.path(directory),
            ink.path(project)
        ),
        protocol::Status::AlreadyHereServingOthers { serving, asked, .. } => format!(
            "a fleet is already here, serving {}; asked for {}",
            ink.paths(serving),
            ink.paths(asked)
        ),
        protocol::Status::InsideFleet { .. }
        | protocol::Status::NotFleetDirectory { .. }
        | protocol::Status::ContainsFleet { .. }
        | protocol::Status::InsideServedProject { .. }
        | protocol::Status::AlreadyServed { .. } => {
            create::placement(status, ink).unwrap_or_default()
        }
        protocol::Status::Missing { path } => format!("{} does not exist", ink.path(path)),
        protocol::Status::Unreadable { path, error } => format!("{}: {error}", ink.path(path)),
        protocol::Status::ConfigDiffers { path, listed } => {
            create::config_differs(path, listed, ink)
        }
        protocol::Status::CannotWrite { path, error } => {
            format!(
                "writing {}: {error}; nothing was registered",
                ink.path(path)
            )
        }
        protocol::Status::BadConfig {
            path,
            line,
            column,
            error,
        } => bad_config(path, *line, *column, error, ink),
        protocol::Status::AlsoClaimed { project, fleet } => format!(
            "{} is also claimed by the fleet in {}",
            ink.path(project),
            ink.path(fleet)
        ),
        protocol::Status::NotServed { path } => format!("no fleet serves {}", ink.path(path)),
        protocol::Status::InsideProject { path, project } => format!(
            "{} is inside the project {}",
            ink.path(path),
            ink.path(project)
        ),
        protocol::Status::NoLongerServed { project, fleet } => format!(
            "the fleet in {} no longer serves {}: its {} dropped it",
            ink.path(fleet),
            ink.path(project),
            ink.accent("fleet.toml")
        ),
        protocol::Status::CannotTurnOn { .. } => cannot_turn_on(status, ink),
        protocol::Status::Malformed { error } => {
            format!("the supervisor could not understand the request: {error}")
        }
        protocol::Status::AdapterFailed { .. }
        | protocol::Status::MultiplexerCouldNotRun { .. }
        | protocol::Status::MultiplexerFailed { .. }
        | protocol::Status::NoSuchSession { .. } => {
            session::refused(status, ink).unwrap_or_default()
        }
        protocol::Status::AdapterUnpinned { .. }
        | protocol::Status::NotAnAdapter { .. }
        | protocol::Status::NoSuchRelease { .. }
        | protocol::Status::BadManifest { .. }
        | protocol::Status::NoBuildFor { .. }
        | protocol::Status::ChecksumMismatch { .. }
        | protocol::Status::ProtocolMismatch { .. }
        | protocol::Status::DownloadFailed { .. }
        | protocol::Status::NoDownloader { .. }
        | protocol::Status::AdapterNotKept { .. }
        | protocol::Status::ReleaseMismatch { .. } => {
            adapters::refused(status, ink).unwrap_or_default()
        }
    }
}

/// A project that could not be turned on because of its fleet's config.
fn cannot_turn_on(status: &protocol::Status, ink: Ink) -> String {
    let protocol::Status::CannotTurnOn {
        project,
        fleet,
        why,
    } = status
    else {
        return String::new();
    };
    format!(
        "{} cannot be turned on: the config of the fleet in {} cannot be used: {}",
        ink.path(project),
        ink.path(fleet),
        refused(why, ink)
    )
}

/// A fleet's config that does not read, and where when the parser knows.
fn bad_config(
    path: &std::path::Path,
    line: Option<usize>,
    column: Option<usize>,
    error: &str,
    ink: Ink,
) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!(
            "{} does not read: line {line}, column {column}: {error}",
            ink.path(path)
        ),
        _ => format!("{} does not read: {error}", ink.path(path)),
    }
}

/// Why the state directory cannot be worked out.
fn path_error(error: &fleet_core::paths::PathError, ink: Ink) -> String {
    match error {
        fleet_core::paths::PathError::NoHome => format!(
            "{} is not set, so there is nowhere to keep state",
            ink.accent("HOME")
        ),
        fleet_core::paths::PathError::NotAbsolute { var, value } => format!(
            "{} must be an absolute path, not {}",
            ink.accent(var),
            ink.path(value)
        ),
    }
}

/// Why the CLI could not go on.
fn cannot(problem: &Problem, ink: Ink) -> String {
    match problem {
        Problem::UnsupportedOs => "tinyfleet runs on macOS and Linux only".into(),
        Problem::StateDir(error) => path_error(error, ink),
        Problem::SocketPathTooLong(path) => format!(
            "the socket path {} is longer than {} bytes",
            ink.path(path),
            protocol::MAX_SOCKET_PATH
        ),
        Problem::NoCurrentDir(error) => format!("cannot read the current directory: {error}"),
        Problem::BadPath { path, error } => format!("{}: {error}", ink.path(path)),
        Problem::NoBinary(error) => format!("cannot find this fleet binary: {error}"),
        Problem::Uninstall { path, error } => uninstall::could_not_remove(path, error, ink),
        Problem::ServiceManager { doing, error } => format!(
            "the service manager did not {} the supervisor: {}",
            match doing {
                Doing::Start => "start",
                Doing::Stop => "stop",
            },
            tool(error, ink)
        ),
    }
}

/// What went wrong with an outside tool.
fn tool(error: &ToolError, ink: Ink) -> String {
    match error {
        ToolError::OwnedElsewhere { theirs, ours } => format!(
            "the installed supervisor serves {}, not {}",
            ink.path(theirs),
            ink.path(ours)
        ),
        ToolError::CouldNotRun { program, error } => {
            format!("cannot run {}: {error}", ink.accent(program))
        }
        ToolError::Failed { command, said } if said.is_empty() => {
            format!("{} failed", ink.accent(command))
        }
        ToolError::Failed { command, said } => format!("{} failed: {said}", ink.accent(command)),
        ToolError::File { doing, path, error } => format!(
            "{} {}: {error}",
            match doing {
                FileStep::Create => "creating",
                FileStep::Secure => "securing",
                FileStep::Write => "writing",
                FileStep::Remove => "removing",
                FileStep::Read => "looking at",
            },
            ink.path(path)
        ),
        ToolError::UnitPath { which, path, why } => format!(
            "the {} path {} {}",
            match which {
                UnitPath::Program => "program",
                UnitPath::StateDir => "state directory",
                UnitPath::Log => "log",
            },
            ink.path(path),
            match why {
                BadUnitPath::NotUtf8 => "is not valid UTF-8",
                BadUnitPath::Newline => "contains a newline",
            }
        ),
    }
}

/// Why the CLI cannot tell whether something happened.
fn unsure(doubt: &Doubt) -> String {
    match doubt {
        Doubt::NoAnswer { waited, .. } => {
            format!("the supervisor did not answer within {}s", waited.as_secs())
        }
        Doubt::StoppedUnanswered { waited, .. } => format!(
            "the supervisor did not answer within {}s; it is stopped, and its fleets were left as they were",
            waited.as_secs()
        ),
        Doubt::StillAnswers { waited } => {
            format!("the supervisor still answers after {}s", waited.as_secs())
        }
        Doubt::Socket { during, error } => {
            format!("{}: {}", during_words(*during), socket(error))
        }
        Doubt::OddReply { to } => format!("the supervisor gave an unexpected answer to {to}"),
    }
}

fn during_words(during: During) -> String {
    match during {
        During::Creating => "asking the supervisor to create the fleet".into(),
        During::Removing => "asking the supervisor to remove the fleet".into(),
        During::TurningOn(startup) => {
            format!("{}, but turning every project on", supervisor_was(startup))
        }
        During::TurningOnProject(startup) => {
            format!("{}, but turning the project on", supervisor_was(startup))
        }
        During::TurningOff => "turning every project off".into(),
        During::TurningOffProject => "turning the project off".into(),
    }
}

/// A failure on the socket, in words.
fn socket(error: &ProtocolError) -> String {
    match error {
        ProtocolError::Io(e) => e.to_string(),
        ProtocolError::Malformed(e) => format!("could not understand the supervisor: {e}"),
        ProtocolError::Empty => "the supervisor closed without answering".into(),
        ProtocolError::TooLong => format!("an answer longer than {} bytes", protocol::MAX_LINE),
    }
}

/// How a command found the supervisor.
fn supervisor_was(startup: Startup) -> &'static str {
    match startup {
        Startup::WasRunning => "the supervisor is already running",
        Startup::Started => "the supervisor is running",
    }
}

/// Said when a command had to start the supervisor itself.
fn started_supervisor() -> String {
    "started the supervisor".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flavour_stays_out_of_failures() {
        let failed = status(
            &Status::CouldNotTell(Doubt::Socket {
                during: During::TurningOn(Startup::Started),
                error: ProtocolError::Empty,
            }),
            crate::ui::output::ERR,
        );
        assert!(!failed.contains("takeoff"), "{failed}");
    }

    #[test]
    fn not_served_sends_nobody_to_create() {
        let status = Status::Refused(protocol::Status::NotServed {
            path: "/code/web".into(),
        });
        assert_eq!(next(&status, crate::ui::output::ERR), None);
    }

    #[test]
    fn a_path_is_quoted_for_the_shell_only_when_it_needs_it() {
        assert_eq!(shell_quoted("/tmp/fleet"), "/tmp/fleet");
        assert_eq!(
            shell_quoted("/Users/a/Library/Application Support/fleet"),
            "'/Users/a/Library/Application Support/fleet'"
        );
        assert_eq!(shell_quoted("/it's"), r"'/it'\''s'");
    }
}
