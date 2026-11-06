//! `fleet remove`: the fleet here leaves the registry, or, with
//! `--project`, those projects leave this wingman fleet.

use std::path::{Path, PathBuf};

use fleet_core::protocol::request::{self, ConfigChoice};
use fleet_core::protocol::{self, response};
use fleet_core::tools::ServiceManager;

use crate::Ask;
use crate::cmd::start;
use crate::protocol::{Problem, Response, Status};
use crate::ui::{self, words};

pub(crate) fn run(
    state_dir: &Path,
    service: &dyn ServiceManager,
    ask: Ask,
    projects: &[PathBuf],
) -> Result<Response, Status> {
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
    let supervisor = start::supervisor(state_dir, service, ask)?;
    // Only a person at a terminal is asked, and only when the supervisor
    // says the answer matters; anyone else keeps the file.
    let first = if ask == Ask::Terminal {
        ConfigChoice::Ask
    } else {
        ConfigChoice::Keep
    };
    let request = |config| {
        protocol::Request::Remove(request::Remove::Fleet {
            directory: directory.clone(),
            projects: absolute.clone(),
            config,
        })
    };
    let mut response = supervisor.ask(&request(first), removed)?;
    if matches!(response, response::Remove::Ask { .. }) {
        let answer = ui::confirm(words::remove::DELETE_PROMPT, false);
        let Some(config) = answered(answer) else {
            return Ok(Response::Cancelled);
        };
        response = supervisor.ask(&request(config), settled)?;
    }
    let startup = supervisor.startup;
    Ok(Response::Removed { response, startup })
}

/// Remove's answer.
fn removed(answer: protocol::Response) -> Option<response::Remove> {
    match answer {
        protocol::Response::Remove(removed) => Some(removed),
        _ => None,
    }
}

/// Remove's answer to the second request, which does not ask first: never
/// a question.
fn settled(answer: protocol::Response) -> Option<response::Remove> {
    removed(answer).filter(|removed| !matches!(removed, response::Remove::Ask { .. }))
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

    #[test]
    fn only_the_first_request_takes_a_question_as_its_answer() {
        let question = || {
            protocol::Response::Remove(response::Remove::Ask {
                directory: "/c/app".into(),
            })
        };
        assert!(removed(question()).is_some());
        assert!(settled(question()).is_none());
        let gone = protocol::Response::Remove(response::Remove::NothingHere {
            directory: "/c/app".into(),
        });
        assert!(settled(gone).is_some());
        assert!(removed(protocol::Response::Stop(response::Stop::AllOff)).is_none());
    }
}
