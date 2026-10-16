//! `fleet start`: the supervisor up, every config re-read, every project on.
//! Also how any command that needs the supervisor brings it up.

use std::path::Path;

use fleet_core::tools::{ServiceManager, SupervisorUnit};

use crate::ending::{Doing, Doubt, Problem, Response, Startup, Status};
use crate::{Ask, client, ui, words};

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
        Ask::Terminal => ui::confirm(words::START_PROMPT, true),
    };
    if !go {
        return Err(Status::NotRunning);
    }
    let program = std::env::current_exe().map_err(|e| Status::Cannot(Problem::NoBinary(e)))?;
    let unit = SupervisorUnit {
        program,
        state_dir: state_dir.to_path_buf(),
    };
    service.enable_and_start(&unit).map_err(|error| {
        Status::Cannot(Problem::ServiceManager {
            doing: Doing::Start,
            error,
        })
    })?;
    let spinner = ui::spinner(words::STARTING);
    let up = client::wait_until(socket, true);
    spinner.clear();
    if !up {
        return Err(Status::CouldNotTell(Doubt::NoAnswer {
            waited: client::WAIT,
            log: service.where_logs_go(state_dir),
        }));
    }
    Ok(Startup::Started)
}
