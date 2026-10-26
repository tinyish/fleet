//! Creating a fleet.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::config::{self, Config};
use fleet_core::fleets::{self, CreateDecision, decide_create};
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::seats::{self, Roster};
use fleet_core::status;
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
    let (asked, roster) = if adopting {
        match adopted(&directory, &canonical) {
            Ok(adopted) => adopted,
            Err(status) => return Ok(Reply::Status(status)),
        }
    } else {
        (canonical, Roster::default())
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
        let moved = seats::diff(&Roster::default(), &roster);
        crate::projects::set_roster(w, &fleet.directory, &roster, &moved)?;
        w.record(&created)
    });
    match written {
        Ok(()) => {}
        Err(state::StateError::Config(e)) => {
            return Ok(Reply::Status(Status::File(status::File::CannotWrite {
                path,
                error: e.to_string(),
            })));
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
/// read as start reads it, and its roles and seats. Projects `asked` for
/// must be the ones it lists. A default fleet's file lists its own
/// directory, which create takes as no projects.
fn adopted(directory: &Path, asked: &[PathBuf]) -> Result<(Vec<PathBuf>, Roster), Status> {
    let (listed, roster) = fleets::read_fleet(
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
            return Err(Status::File(status::File::ConfigDiffers {
                path: directory.join(fleet_core::paths::CONFIG_FILE),
                listed,
            }));
        }
    }
    if listed == [directory] {
        return Ok((Vec::new(), roster));
    }
    Ok((listed, roster))
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
            Some(Reply::Status(Status::File(status::File::ConfigDiffers {
                path: wing.join("fleet.toml"),
                listed: vec![code(&root, "api"), code(&root, "blog")],
            })))
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
            matches!(
                &reply,
                Some(Reply::Status(Status::File(status::File::BadConfig { .. })))
            ),
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
            matches!(
                &reply,
                Some(Reply::Status(Status::Fleet(
                    status::Fleet::AlreadyServed { .. }
                )))
            ),
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

    fn seats_of(state: &Mutex<State>, fleet: &Path) -> Vec<String> {
        let roster = lock(state).roster(fleet).unwrap_or_default();
        roster.seats.into_iter().map(|s| s.name).collect()
    }

    #[test]
    fn an_adopted_file_brings_its_roles_and_seats() {
        let (root, state) = tree("adopt-seats", &["app"]);
        let app = code(&root, "app");
        let _ = fs::write(
            app.join("fleet.toml"),
            "[roles.w]\nadapter = \"/a/claude\"\n[seats.scribe]\nrole = \"w\"\n",
        );
        assert!(created(create(&state, &app, &[]).ok()).is_some());
        assert_eq!(seats_of(&state, &app), ["scribe"]);
        let steps = lock(&state).steps().unwrap_or_default();
        assert_eq!(steps, ["ROLE_ADDED", "SEAT_ADDED", "FLEET_CREATED"]);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_adopted_file_whose_seats_break_a_rule_is_refused() {
        let (root, state) = tree("adopt-bad-seat", &["app"]);
        let app = code(&root, "app");
        let _ = fs::write(app.join("fleet.toml"), "[seats.scribe]\nrole = \"w\"\n");
        let reply = create(&state, &app, &[]).ok();
        assert!(
            matches!(
                &reply,
                Some(Reply::Status(Status::File(
                    status::File::UnknownRole { .. }
                )))
            ),
            "{reply:?}"
        );
        assert_eq!(fleets(&state), Vec::<Fleet>::new());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_fresh_fleet_has_no_seats() {
        let (root, state) = tree("fresh-seats", &["app"]);
        let app = code(&root, "app");
        assert!(created(create(&state, &app, &[]).ok()).is_some());
        assert_eq!(seats_of(&state, &app), Vec::<String>::new());
        let _ = fs::remove_dir_all(root);
    }
}
