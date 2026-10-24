//! `fleet remove`: the fleet here leaves the registry, or, with
//! `--project`, those projects leave this wingman fleet.

use std::path::{Path, PathBuf};

use fleet_core::protocol::request::{self, ConfigChoice};
use fleet_core::protocol::response;
use fleet_core::tools::ServiceManager;

use crate::cmd::start;
use crate::protocol::{Doubt, Problem, Response, Status};
use crate::ui::{self, words};
use crate::{Ask, client};

pub(crate) fn run(
    state_dir: &Path,
    service: &dyn ServiceManager,
    ask: Ask,
    projects: &[PathBuf],
) -> Result<Response, Status> {
    let socket = client::socket(state_dir)?;
    let directory =
        std::env::current_dir().map_err(|e| Status::Cannot(Problem::NoCurrentDir(e)))?;
    let mut absolute = Vec::with_capacity(projects.len());
    for project in projects {
        let path = std::path::absolute(project).map_err(|error| {
            Status::Cannot(Problem::BadPath {
                path: project.clone(),
                error,
            })
        })?;
        absolute.push(path);
    }
    let startup = start::ensure_supervisor(state_dir, &socket, service, ask)?;
    // Only a person at a terminal is asked, and only when the supervisor
    // says the answer matters; anyone else keeps the file.
    let first = if ask == Ask::Terminal {
        ConfigChoice::Ask
    } else {
        ConfigChoice::Keep
    };
    let request = |config| request::Remove::Fleet {
        directory: directory.clone(),
        projects: absolute.clone(),
        config,
    };
    let mut response = client::remove(&socket, request(first))?;
    if matches!(response, response::Remove::Ask { .. }) {
        let answer = ui::confirm(words::remove::DELETE_PROMPT, false);
        let Some(config) = answered(answer) else {
            return Ok(Response::Cancelled);
        };
        response = client::remove(&socket, request(config))?;
        if matches!(response, response::Remove::Ask { .. }) {
            return Err(Status::CouldNotTell(Doubt::OddReply { to: "remove" }));
        }
    }
    Ok(Response::Removed { response, startup })
}

/// What the second request says, from the person's answer: nothing is sent
/// when they cancelled, since "no" would still remove the fleet.
fn answered(answer: Option<bool>) -> Option<ConfigChoice> {
    answer.map(|delete| {
        if delete {
            ConfigChoice::Delete
        } else {
            ConfigChoice::Keep
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_question_sends_nothing_and_an_answer_picks_the_choice() {
        assert_eq!(answered(None), None);
        assert_eq!(answered(Some(true)), Some(ConfigChoice::Delete));
        assert_eq!(answered(Some(false)), Some(ConfigChoice::Keep));
    }
}
