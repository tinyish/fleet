//! Every sentence tinyfleet says to a person. The supervisor replies with
//! facts, `protocol::Response` or `protocol::Status`; a command ends with the
//! CLI's own `Response` or `Status`. This file turns all of them into words
//! (core.md invariant 9), colouring the variable parts with the ink of the
//! stream they are written to.

use fleet_core::tools::{BadUnitPath, FileStep, ToolError, UnitPath};
use protocol::ProtocolError;

use crate::ending::{Doing, Doubt, During, Problem, Response, Startup, Status};
use crate::output::Ink;

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

/// The help for `fleet start`.
pub(crate) const START_ABOUT: &str =
    "Start the supervisor and turn every fleet on; it comes back after a reboot until stop";

/// The help for `fleet stop`.
pub(crate) const STOP_ABOUT: &str =
    "Turn every fleet off and stop the supervisor, so it does not come back";

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

/// What a command did.
pub(crate) fn response(response: &Response, ink: Ink) -> String {
    match response {
        Response::Created {
            response,
            startup: Startup::Started,
        } => format!("{}; {}", started_supervisor(ink), created(response, ink)),
        Response::Created { response, .. } => created(response, ink),
        Response::Started { response, startup } => {
            format!("{}; {}", supervisor_was(*startup), started(response))
        }
        Response::SupervisorStopped => "every fleet is off; the supervisor is stopped".into(),
        Response::SupervisorWasNotRunning => "the supervisor was not running".into(),
    }
}

/// Why a command did not do what was asked.
pub(crate) fn status(status: &Status, ink: Ink) -> String {
    match status {
        Status::Refused(status) => refused(status, ink),
        Status::Cannot(problem) => cannot(problem, ink),
        Status::NotRunning => "the supervisor is not running".into(),
        Status::CouldNotTell(doubt) => unsure(doubt),
    }
}

/// What to do about a status, when there is something to do: the hint
/// line printed under it.
pub(crate) fn next(status: &Status, ink: Ink) -> Option<String> {
    let create = ink.accent("fleet create");
    let project_flag = ink.accent("--project");
    Some(match status {
        Status::NotRunning => format!("run {} to start it", ink.accent("fleet start")),
        Status::Refused(protocol::Status::DirectoryNotInRepo { .. }) => format!(
            "run {create} at a repository's root, or pass {project_flag} for a wingman fleet"
        ),
        Status::Refused(protocol::Status::DirectoryNotRepoRoot { root, .. }) => format!(
            "run {create} in {}, or pass {project_flag} for a wingman fleet",
            ink.path(root)
        ),
        Status::Refused(protocol::Status::ProjectNotRepoRoot { root, .. }) => {
            format!("pass the repository's root, {}, instead", ink.path(root))
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
        Status::CouldNotTell(Doubt::NoAnswer { log, .. }) => {
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

/// What the supervisor said create did.
fn created(response: &protocol::response::Create, ink: Ink) -> String {
    let protocol::response::Create {
        directory,
        projects,
        created,
    } = response;
    if !created {
        return format!(
            "a fleet is already here, serving {}; nothing changed",
            ink.paths(projects)
        );
    }
    if projects.len() == 1 && projects[0] == *directory {
        return format!(
            "created a fleet in {}; {} is uncommitted",
            ink.path(directory),
            ink.accent("fleet.toml")
        );
    }
    format!(
        "created a wingman fleet in {} serving {}",
        ink.path(directory),
        ink.paths(projects)
    )
}

/// What the supervisor said start did.
fn started(response: &protocol::response::Start) -> String {
    match response {
        protocol::response::Start::AllOn => "every fleet is on".into(),
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
            format!("{} is this directory", ink.path(path))
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
        protocol::Status::InsideFleet { directory, fleet } => format!(
            "{} is inside the fleet in {}",
            ink.path(directory),
            ink.path(fleet)
        ),
        protocol::Status::ContainsFleet { directory, fleet } => format!(
            "{} contains the fleet in {}",
            ink.path(directory),
            ink.path(fleet)
        ),
        protocol::Status::InsideServedProject {
            directory,
            project,
            fleet,
        } => format!(
            "{} is inside {}, which the fleet in {} serves",
            ink.path(directory),
            ink.path(project),
            ink.path(fleet)
        ),
        protocol::Status::AlreadyServed { project, fleet } => format!(
            "{} is already served by the fleet in {}",
            ink.path(project),
            ink.path(fleet)
        ),
        protocol::Status::Missing { path } => format!("{} does not exist", ink.path(path)),
        protocol::Status::Unreadable { path, error } => format!("{}: {error}", ink.path(path)),
        protocol::Status::StrayConfig { path } => format!(
            "{} is already here and no fleet owns it; it was left alone",
            ink.path(path)
        ),
        protocol::Status::CannotWrite { path, error } => {
            format!(
                "writing {}: {error}; nothing was registered",
                ink.path(path)
            )
        }
        protocol::Status::Malformed { error } => {
            format!("the supervisor could not understand the request: {error}")
        }
    }
}

/// Why the CLI could not go on.
fn cannot(problem: &Problem, ink: Ink) -> String {
    match problem {
        Problem::UnsupportedOs => "tinyfleet runs on macOS and Linux only".into(),
        Problem::StateDir(fleet_core::paths::PathError::NoHome) => format!(
            "{} is not set, so there is nowhere to keep state",
            ink.accent("HOME")
        ),
        Problem::StateDir(fleet_core::paths::PathError::NotAbsolute { var, value }) => format!(
            "{} must be an absolute path, not {}",
            ink.accent(var),
            ink.path(value)
        ),
        Problem::SocketPathTooLong(path) => format!(
            "the socket path {} is longer than {} bytes",
            ink.path(path),
            protocol::MAX_SOCKET_PATH
        ),
        Problem::NoCurrentDir(error) => format!("cannot read the current directory: {error}"),
        Problem::BadPath { path, error } => format!("{}: {error}", ink.path(path)),
        Problem::NoBinary(error) => format!("cannot find this fleet binary: {error}"),
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
        During::TurningOn(startup) => {
            format!("{}, but turning every fleet on", supervisor_was(startup))
        }
        During::TurningOff => "turning every fleet off".into(),
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
        Startup::WasRunning => "the supervisor was already running",
        Startup::Started => "the supervisor is running",
    }
}

/// Said when a command had to start the supervisor itself.
fn started_supervisor(ink: Ink) -> String {
    format!(
        "started the supervisor (it comes back after a reboot until {})",
        ink.accent("fleet stop")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
