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
mod file;
mod fleet;
mod message;
mod overview;
mod project;
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

/// The help for `fleet status`.
pub(crate) const STATUS_ABOUT: &str = "Show each seat of the fleet here, or of every fleet, \
     and what its agent is doing";

/// The help for `fleet message`.
pub(crate) const MESSAGE_ABOUT: &str = "Send text to a seat of the fleet here, or to a session \
     by its id; it is typed in once the agent is idle";

/// The help for `fleet message`'s recipient.
pub(crate) const MESSAGE_TO_HELP: &str = "A seat of the fleet here, or a session's id";

/// The help for `fleet message`'s text.
pub(crate) const MESSAGE_TEXT_HELP: &str = "What to send; without it, what is piped in";

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
        Response::Overview {
            overview,
            now_millis,
        } => overview::lines(overview, *now_millis, ink),
        Response::MessageQueued(queued) => message::queued(queued, ink),
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
        Response::Overview { overview, .. }
            if overview.fleets.is_empty() && overview.loose.is_empty() =>
        {
            Some(format!(
                "run {} in a repository to make one",
                ink.accent("fleet create")
            ))
        }
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
    match status {
        Status::Refused(refusal) => refused_hint(refusal, ink),
        Status::Cannot(problem) => cannot_hint(problem, ink),
        Status::NotRunning => Some(format!("run {} to start it", ink.accent("fleet start"))),
        Status::NeedsAnswer => Some(uninstall::add_flag(ink)),
        Status::PartlyOn { .. } => Some(format!(
            "fix each {} named above, then run {}",
            ink.accent("fleet.toml"),
            ink.accent("fleet start")
        )),
        Status::CouldNotTell(doubt) => match doubt {
            Doubt::NoAnswer { log, .. } | Doubt::StoppedUnanswered { log, .. } => {
                Some(format!("check its log: {}", ink.accent(log)))
            }
            Doubt::StillAnswers { .. } | Doubt::Socket { .. } | Doubt::OddReply { .. } => None,
        },
    }
}

/// The hint for a refusal, from its area.
fn refused_hint(status: &protocol::Status, ink: Ink) -> Option<String> {
    match status {
        protocol::Status::Fleet(refusal) => fleet::hint(refusal, ink),
        protocol::Status::Project(refusal) => project::hint(refusal, ink),
        protocol::Status::File(refusal) => file::hint(refusal, ink),
        protocol::Status::Session(refusal) => session::hint(refusal, ink),
        protocol::Status::Adapter(refusal) => adapters::hint(refusal, ink),
        protocol::Status::Message(refusal) => message::hint(refusal, ink),
        protocol::Status::Malformed { .. } => None,
    }
}

/// The hint for a problem on the CLI's side.
fn cannot_hint(problem: &Problem, ink: Ink) -> Option<String> {
    match problem {
        Problem::Uninstall { .. } => Some(uninstall::try_again(ink)),
        Problem::SocketPathTooLong(_) => Some(format!(
            "set {} to a shorter directory",
            ink.accent("FLEET_STATE_DIR")
        )),
        Problem::ServiceManager {
            error: ToolError::OwnedElsewhere { theirs, .. },
            ..
        } => Some(format!(
            "run {} first to stop it",
            ink.accent(format!(
                "FLEET_STATE_DIR={} fleet stop",
                shell_quoted(&theirs.display().to_string())
            ))
        )),
        Problem::NoText => Some("give the text after the seat or session, or pipe it in".into()),
        Problem::ServiceManager { .. }
        | Problem::UnsupportedOs
        | Problem::StateDir(_)
        | Problem::NoCurrentDir(_)
        | Problem::ReadText(_)
        | Problem::BadPath { .. }
        | Problem::NoBinary(_) => None,
    }
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

/// Why the supervisor said no, in its area's words.
fn refused(status: &protocol::Status, ink: Ink) -> String {
    match status {
        protocol::Status::Fleet(refusal) => fleet::refused(refusal, ink),
        protocol::Status::Project(refusal) => project::refused(refusal, ink),
        protocol::Status::File(refusal) => file::refused(refusal, ink),
        protocol::Status::Session(refusal) => session::refused(refusal, ink),
        protocol::Status::Adapter(refusal) => adapters::refused(refusal, ink),
        protocol::Status::Message(refusal) => message::refused(refusal, ink),
        protocol::Status::Malformed { error } => {
            format!("the supervisor could not understand the request: {error}")
        }
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
        Problem::NoText => "there is no message to send".into(),
        Problem::ReadText(error) => format!("cannot read the message piped in: {error}"),
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
            "the {} {} {}",
            match which {
                UnitPath::Program => "program path",
                UnitPath::StateDir => "state directory path",
                UnitPath::Log => "log path",
                UnitPath::SearchPath => "PATH",
            },
            ink.path(path),
            match why {
                BadUnitPath::NotUtf8 => "is not valid UTF-8",
                BadUnitPath::Newline => "contains a newline",
            }
        ),
        ToolError::TooLong { bytes, limit } => {
            format!("the command would be {bytes} bytes, more than the {limit} it takes")
        }
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
        During::Looking => "asking the supervisor what its agents are doing".into(),
        During::Messaging => "asking the supervisor to queue the message".into(),
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
        let status = Status::Refused(
            fleet_core::status::Project::NotServed {
                path: "/code/web".into(),
            }
            .into(),
        );
        assert_eq!(next(&status, crate::ui::output::ERR), None);
    }

    #[test]
    fn a_hint_moved_to_its_area_still_shows() {
        let status = Status::Refused(
            fleet_core::status::Fleet::DirectoryNotRepoRoot {
                path: "/code/app/src".into(),
                root: "/code/app".into(),
            }
            .into(),
        );
        let hint = next(&status, crate::ui::output::ERR.piped())
            .map(|h| console::strip_ansi_codes(&h).into_owned());
        assert_eq!(hint.as_deref(), Some("run fleet create in /code/app"));
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
