//! `fleet start`: the supervisor up, every config re-read, every project on.
//! Also how any command that needs the supervisor brings it up.

use std::ffi::OsString;
use std::path::Path;

use fleet_core::tools::{ServiceManager, SupervisorUnit};

use crate::protocol::{Doing, Doubt, Problem, Response, Startup, Status};
use crate::ui::{self, words};
use crate::{Ask, client};

pub(crate) fn run(state_dir: &Path, service: &dyn ServiceManager) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    let startup = ensure_supervisor(state_dir, &socket, service, Ask::Yes)?;
    let response = client::start_all(&socket, startup)?;
    if response.stayed_off.is_empty() {
        Ok(Response::Started { response, startup })
    } else {
        Err(Status::PartlyOn { response, startup })
    }
}

/// Makes sure the supervisor answers on `socket`, starting it when `ask`
/// allows, and says how it found it. Starting it here does not turn any
/// project on.
pub(crate) fn ensure_supervisor(
    state_dir: &Path,
    socket: &Path,
    service: &dyn ServiceManager,
    ask: Ask,
) -> Result<Startup, Status> {
    if client::answers(socket) {
        return Ok(Startup::WasRunning);
    }
    let go = match ask {
        Ask::Yes => true,
        Ask::Never => false,
        // A cancel is a no: nothing starts.
        Ask::Terminal => ui::confirm(words::START_PROMPT, true).unwrap_or(false),
    };
    if !go {
        return Err(Status::NotRunning);
    }
    let program = std::env::current_exe().map_err(|e| Status::Cannot(Problem::NoBinary(e)))?;
    let unit = SupervisorUnit {
        program,
        state_dir: state_dir.to_path_buf(),
        env: carried(std::env::vars_os()),
    };
    service.enable_and_start(&unit).map_err(|error| {
        Status::Cannot(Problem::ServiceManager {
            doing: Doing::Start,
            error,
        })
    })?;
    let spinner = ui::spinner(words::STARTING);
    let up = client::wait_until(socket, true, client::WAIT);
    spinner.clear();
    if !up {
        return Err(Status::CouldNotTell(Doubt::NoAnswer {
            waited: client::WAIT,
            log: service.where_logs_go(state_dir),
        }));
    }
    Ok(Startup::Started)
}

/// Of `vars`, what the supervisor's unit carries: `PATH`, `LANG` and every
/// `LC_*`, sorted by name.
fn carried(vars: impl Iterator<Item = (OsString, OsString)>) -> Vec<(String, OsString)> {
    let mut env: Vec<_> = vars
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value)))
        .filter(|(name, _)| name == "PATH" || name == "LANG" || name.starts_with("LC_"))
        .collect();
    env.sort();
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_unit_carries_the_path_and_the_locale_only() {
        let vars = [
            ("PATH", "/opt/homebrew/bin:/usr/bin"),
            ("HOME", "/Users/you"),
            ("LC_CTYPE", "UTF-8"),
            ("LANGUAGE", "en"),
            ("LANG", "en_US.UTF-8"),
            ("LC_ALL", "C"),
        ]
        .map(|(n, v)| (OsString::from(n), OsString::from(v)));
        let names: Vec<String> = carried(vars.into_iter())
            .into_iter()
            .map(|(n, _)| n)
            .collect();
        assert_eq!(names, ["LANG", "LC_ALL", "LC_CTYPE", "PATH"]);
    }
}
