//! Creating a fleet.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::config::{self, Config};
use fleet_core::fleets::{self, CreateDecision, decide_create};
use fleet_core::protocol::{Reply, Response, Status, response};
use state::{State, Step};

use crate::{Error, lock, repo_root_of};

/// Creates a fleet, under the state lock so that two creates never race.
pub(crate) fn create(
    state: &Mutex<State>,
    directory: &Path,
    projects: &[PathBuf],
) -> Result<Reply, Error> {
    let directory = match fs::canonicalize(directory) {
        Ok(d) => d,
        Err(e) => return Ok(Reply::Status(Status::unusable(directory, &e))),
    };
    let mut canonical = Vec::with_capacity(projects.len());
    for project in projects {
        match fs::canonicalize(project) {
            Ok(p) => canonical.push(p),
            Err(e) => return Ok(Reply::Status(Status::unusable(project, &e))),
        }
    }
    let mut state = lock(state);
    let registered = state.fleets()?;
    let path = directory.join(fleet_core::paths::CONFIG_FILE);
    // A file no fleet owns is adopted: the fleet is the one it describes.
    let adopting = path.exists() && !registered.iter().any(|f| f.directory == directory);
    let asked = if adopting {
        match adopted_projects(&directory, &canonical) {
            Ok(projects) => projects,
            Err(status) => return Ok(Reply::Status(status)),
        }
    } else {
        canonical
    };
    let fleet = match decide_create(&registered, &directory, &asked, repo_root_of) {
        CreateDecision::Create(fleet) => fleet,
        CreateDecision::AlreadyHere(fleet) => {
            return Ok(Reply::Response(Response::Create(response::Create {
                directory: fleet.directory,
                projects: fleet.projects,
                created: false,
                adopted: false,
            })));
        }
        CreateDecision::Refuse(status) => return Ok(Reply::Status(status)),
    };
    let text = config::render(&Config::for_fleet(&fleet));
    let created = Step::FleetCreated {
        directory: directory.display().to_string(),
        projects: fleet
            .projects
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        adopted: adopting,
    };
    let written = state.write(|w| {
        w.create_fleet(&fleet, || {
            if adopting {
                return Ok(());
            }
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)?
                .write_all(text.as_bytes())
        })?;
        w.record(&created)
    });
    match written {
        Ok(()) => {}
        Err(state::StateError::Config(e)) => {
            return Ok(Reply::Status(Status::CannotWrite {
                path,
                error: e.to_string(),
            }));
        }
        Err(e) => return Err(e.into()),
    }
    Ok(Reply::Response(Response::Create(response::Create {
        directory,
        projects: fleet.projects,
        created: true,
        adopted: adopting,
    })))
}

/// The projects to create from the `fleet.toml` already in `directory`,
/// read as start reads it. Projects `asked` for must be the ones it lists.
/// A default fleet's file lists its own directory, which create takes as no
/// projects.
fn adopted_projects(directory: &Path, asked: &[PathBuf]) -> Result<Vec<PathBuf>, Status> {
    let listed = fleets::read_projects(
        directory,
        |p| fs::read_to_string(p),
        |p| fs::canonicalize(p),
        repo_root_of,
    )?;
    if !asked.is_empty() {
        let mut asked = asked.to_vec();
        asked.sort();
        asked.dedup();
        if asked != listed {
            return Err(Status::ConfigDiffers {
                path: directory.join(fleet_core::paths::CONFIG_FILE),
                listed,
            });
        }
    }
    if listed == [directory] {
        return Ok(Vec::new());
    }
    Ok(listed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{code, tree};
    use fleet_core::fleets::Fleet;

    fn created(reply: Option<Reply>) -> Option<response::Create> {
        match reply {
            Some(Reply::Response(Response::Create(c))) => Some(c),
            _ => None,
        }
    }

    fn fleets(state: &Mutex<State>) -> Vec<Fleet> {
        lock(state).fleets().unwrap_or_default()
    }

    #[test]
    fn a_default_fleet_toml_left_behind_is_adopted_as_it_is() {
        let (root, state) = tree("adopt-default", &["app"]);
        let app = code(&root, "app");
        let text = "# kept by fleet remove\n";
        let _ = fs::write(app.join("fleet.toml"), text);
        let made = created(create(&state, &app, &[]).ok());
        assert_eq!(
            made.as_ref().map(|c| (c.created, c.adopted)),
            Some((true, true))
        );
        assert_eq!(made.map(|c| c.projects), Some(vec![app.clone()]));
        assert_eq!(
            fs::read_to_string(app.join("fleet.toml")).ok().as_deref(),
            Some(text)
        );
        assert_eq!(fleets(&state).len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_hand_written_wingman_file_is_adopted_and_a_matching_project_list_too() {
        let (root, state) = tree("adopt-wing", &["api", "blog"]);
        let wing = root.join("wing");
        let text = "# mine\nprojects = [\"../code/blog\", # blog\n  \"../code/api\"]\n";
        let _ = fs::write(wing.join("fleet.toml"), text);
        let asked = [code(&root, "api"), wing.join("../code/blog")];
        let made = created(create(&state, &wing, &asked).ok());
        assert_eq!(made.as_ref().map(|c| c.adopted), Some(true));
        assert_eq!(
            made.map(|c| c.projects),
            Some(vec![code(&root, "api"), code(&root, "blog")])
        );
        assert_eq!(
            fs::read_to_string(wing.join("fleet.toml")).ok().as_deref(),
            Some(text)
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_project_list_that_differs_from_the_file_is_refused_and_changes_nothing() {
        let (root, state) = tree("differs", &["api", "blog"]);
        let wing = root.join("wing");
        let _ = fs::write(
            wing.join("fleet.toml"),
            "projects = [\"../code/api\", \"../code/blog\"]\n",
        );
        let reply = create(&state, &wing, &[code(&root, "api")]).ok();
        assert_eq!(
            reply,
            Some(Reply::Status(Status::ConfigDiffers {
                path: wing.join("fleet.toml"),
                listed: vec![code(&root, "api"), code(&root, "blog")],
            }))
        );
        assert_eq!(fleets(&state), Vec::<Fleet>::new());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_that_does_not_read_is_refused_for_what_is_wrong() {
        let (root, state) = tree("broken", &["app"]);
        let app = code(&root, "app");
        let _ = fs::write(app.join("fleet.toml"), "projects = [");
        let reply = create(&state, &app, &[]).ok();
        assert!(
            matches!(&reply, Some(Reply::Status(Status::BadConfig { .. }))),
            "{reply:?}"
        );
        assert_eq!(fleets(&state), Vec::<Fleet>::new());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_listing_a_project_another_fleet_serves_is_refused() {
        let (root, state) = tree("served", &["api"]);
        let api = code(&root, "api");
        assert!(created(create(&state, &api, &[]).ok()).is_some());
        let wing = root.join("wing");
        let _ = fs::write(wing.join("fleet.toml"), "projects = [\"../code/api\"]\n");
        let reply = create(&state, &wing, &[]).ok();
        assert!(
            matches!(&reply, Some(Reply::Status(Status::AlreadyServed { .. }))),
            "{reply:?}"
        );
        assert_eq!(fleets(&state).len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_new_fleet_still_writes_its_file_and_is_not_adopted() {
        let (root, state) = tree("fresh", &["app"]);
        let app = code(&root, "app");
        let made = created(create(&state, &app, &[]).ok());
        assert_eq!(made.map(|c| c.adopted), Some(false));
        assert!(app.join("fleet.toml").exists());
        let _ = fs::remove_dir_all(root);
    }
}
