//! Removing a fleet, or taking projects out of a wingman fleet.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use fleet_core::config;
use fleet_core::fleets::{Fleet, RemoveDecision, decide_remove};
use fleet_core::paths::CONFIG_FILE;
use fleet_core::protocol::request::ConfigChoice;
use fleet_core::protocol::response::{self, ConfigFate};
use fleet_core::protocol::{Reply, Response, Status};
use fleet_core::status;
use state::{State, StateError, Step};

use crate::{Error, lock};

/// Removes the fleet in `directory`, or takes `projects` out of it, under
/// the state lock so it never races a create or a start. A path that no
/// longer exists is taken as given, as `stop --project` does, so a fleet can
/// be removed after its repositories are gone.
pub(crate) fn remove(
    state: &Mutex<State>,
    directory: &Path,
    projects: &[PathBuf],
    config: ConfigChoice,
) -> Result<Reply, Error> {
    let directory = canonical(directory);
    let named: Vec<PathBuf> = projects.iter().map(|p| canonical(p)).collect();
    let mut state = lock(state);
    let registered = state.fleets()?;
    match decide_remove(&registered, &directory, &named) {
        RemoveDecision::NothingHere => Ok(answer(response::Remove::NothingHere { directory })),
        RemoveDecision::Refuse(status) => Ok(Reply::Status(status)),
        RemoveDecision::Whole(fleet) => whole(&mut state, fleet, config),
        RemoveDecision::Take { fleet, taken } => take(&mut state, fleet, taken),
    }
}

/// `path` made canonical; for one that no longer exists, its `..` resolved
/// by text and its nearest existing ancestor made canonical, so a deleted
/// repository written `../blog` still names the project it was.
fn canonical(path: &Path) -> PathBuf {
    if let Ok(path) = fs::canonicalize(path) {
        return path;
    }
    let lexical = fleet_core::paths::lexical(path);
    lexical
        .ancestors()
        .find_map(|base| {
            let real = fs::canonicalize(base).ok()?;
            let rest = lexical.strip_prefix(base).ok()?;
            Some(real.join(rest))
        })
        .unwrap_or(lexical)
}

fn answer(remove: response::Remove) -> Reply {
    Reply::Response(Response::Remove(remove))
}

fn shown(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|p| p.display().to_string()).collect()
}

/// The whole fleet goes; its config file too when `choice` says to delete
/// it. Asked to ask first while the file is there, it changes nothing and
/// answers `Ask`. The file is deleted inside the write, so a delete that
/// fails changes nothing.
fn whole(state: &mut State, fleet: Fleet, choice: ConfigChoice) -> Result<Reply, Error> {
    let path = fleet.directory.join(CONFIG_FILE);
    let found = if path.exists() {
        ConfigFate::Kept
    } else {
        ConfigFate::Missing
    };
    if found == ConfigFate::Kept && choice == ConfigChoice::Ask {
        return Ok(answer(response::Remove::Ask {
            directory: fleet.directory,
        }));
    }
    let delete_config = choice == ConfigChoice::Delete;
    let written = state.write(|w| {
        w.remove_fleet(&fleet.directory)?;
        let config = match found {
            ConfigFate::Kept if delete_config => match fs::remove_file(&path) {
                Ok(()) => ConfigFate::Deleted,
                Err(e) if e.kind() == io::ErrorKind::NotFound => ConfigFate::Missing,
                Err(e) => return Err(StateError::Config(e)),
            },
            other => other,
        };
        w.record(&Step::FleetRemoved {
            directory: fleet.directory.display().to_string(),
            projects: shown(&fleet.projects),
            config,
        })?;
        crate::messages::fleet_leaves(w, &fleet.directory)?;
        Ok(config)
    });
    let config = match written {
        Ok(config) => config,
        Err(StateError::Config(e)) => return Ok(cannot_write(path, &e)),
        Err(e) => return Err(e.into()),
    };
    Ok(answer(response::Remove::Removed {
        directory: fleet.directory,
        projects: fleet.projects,
        config,
    }))
}

/// Takes `taken` out of the wingman fleet: its config file loses their
/// entries, kept as the person wrote the rest, and the registry loses them.
fn take(state: &mut State, fleet: Fleet, taken: Vec<PathBuf>) -> Result<Reply, Error> {
    let path = fleet.directory.join(CONFIG_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return Ok(Reply::Status(Status::unusable(&path, &e))),
    };
    let directory = &fleet.directory;
    let edited = config::without_projects(&text, |entry| {
        taken.contains(&canonical(&directory.join(entry)))
    });
    let edited = match edited {
        Ok(edited) => edited,
        Err(e) => {
            return Ok(Reply::Status(Status::File(status::File::BadConfig {
                path,
                line: e.line,
                column: e.column,
                error: e.message,
            })));
        }
    };
    let written = state.write(|w| {
        w.remove_projects(&taken)?;
        replace(&path, &edited).map_err(StateError::Config)?;
        w.record(&Step::ProjectsTaken {
            fleet: directory.display().to_string(),
            projects: shown(&taken),
        })
    });
    match written {
        Ok(()) => {}
        Err(StateError::Config(e)) => return Ok(cannot_write(path, &e)),
        Err(e) => return Err(e.into()),
    }
    let remaining = fleet
        .projects
        .iter()
        .filter(|p| !taken.contains(p))
        .cloned()
        .collect();
    Ok(answer(response::Remove::ProjectsTaken {
        directory: fleet.directory,
        taken,
        remaining,
    }))
}

/// Writes `text` over `path` through a temporary file and a rename, so the
/// file is never half written.
fn replace(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("toml.tmp");
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)
}

fn cannot_write(path: PathBuf, error: &io::Error) -> Reply {
    Reply::Status(Status::File(status::File::CannotWrite {
        path,
        error: error.to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    use crate::create::create;
    use crate::testing::{code, tree};

    fn created(state: &Mutex<State>, directory: &Path, projects: &[PathBuf]) {
        let reply = create(state, directory, projects).ok();
        assert!(
            matches!(reply, Some(Reply::Response(Response::Create(_)))),
            "{reply:?}"
        );
    }

    fn removed(
        state: &Mutex<State>,
        directory: &Path,
        projects: &[PathBuf],
        delete: bool,
    ) -> Option<Reply> {
        let config = if delete {
            ConfigChoice::Delete
        } else {
            ConfigChoice::Keep
        };
        remove(state, directory, projects, config).ok()
    }

    fn fleets(state: &Mutex<State>) -> usize {
        lock(state).fleets().map_or(usize::MAX, |f| f.len())
    }

    #[test]
    fn a_whole_remove_keeps_or_deletes_fleet_toml() {
        let (root, state) = tree("whole", &["app"]);
        let app = code(&root, "app");
        let config = app.join("fleet.toml");
        created(&state, &app, &[]);
        let reply = removed(&state, &app, &[], false);
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::Removed {
                        config: response::ConfigFate::Kept,
                        ..
                    }
                )))
            ),
            "{reply:?}"
        );
        assert!(config.exists());
        assert_eq!(fleets(&state), 0);
        let _ = fs::remove_file(&config);
        created(&state, &app, &[]);
        let reply = removed(&state, &app, &[], true);
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::Removed {
                        config: response::ConfigFate::Deleted,
                        ..
                    }
                )))
            ),
            "{reply:?}"
        );
        assert!(!config.exists());
        assert_eq!(fleets(&state), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn removing_twice_is_nothing_here() {
        let (root, state) = tree("twice", &["app"]);
        let app = code(&root, "app");
        created(&state, &app, &[]);
        let _ = removed(&state, &app, &[], false);
        let reply = removed(&state, &app, &[], true);
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::NothingHere { .. }
                )))
            ),
            "{reply:?}"
        );
        assert!(app.join("fleet.toml").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_take_edits_fleet_toml_and_keeps_the_fleet() {
        let (root, state) = tree("take", &["api", "blog"]);
        let wing = root.join("wing");
        let (api, blog) = (code(&root, "api"), code(&root, "blog"));
        created(&state, &wing, &[api.clone(), blog.clone()]);
        let reply = removed(&state, &wing, std::slice::from_ref(&blog), true);
        assert_eq!(
            reply,
            Some(Reply::Response(Response::Remove(
                response::Remove::ProjectsTaken {
                    directory: wing.clone(),
                    taken: vec![blog.clone()],
                    remaining: vec![api.clone()],
                }
            )))
        );
        let text = fs::read_to_string(wing.join("fleet.toml")).unwrap_or_default();
        assert!(!text.contains("blog") && text.contains("api"), "{text}");
        let left = lock(&state).fleets().ok();
        assert_eq!(
            left.map(|f| f.iter().map(|f| f.projects.clone()).collect::<Vec<_>>()),
            Some(vec![vec![api]])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn taking_the_last_project_removes_the_fleet() {
        let (root, state) = tree("last", &["api"]);
        let wing = root.join("wing");
        let api = code(&root, "api");
        created(&state, &wing, std::slice::from_ref(&api));
        let reply = removed(&state, &wing, &[api], false);
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::Removed {
                        config: response::ConfigFate::Kept,
                        ..
                    }
                )))
            ),
            "{reply:?}"
        );
        assert_eq!(fleets(&state), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn inside_a_fleet_is_refused_and_changes_nothing() {
        let (root, state) = tree("inside", &["app"]);
        let app = code(&root, "app");
        let src = app.join("src");
        let _ = fs::create_dir_all(&src);
        created(&state, &app, &[]);
        let reply = removed(&state, &src, &[], true);
        assert_eq!(
            reply,
            Some(Reply::Status(Status::Fleet(
                status::Fleet::NotFleetDirectory {
                    directory: src,
                    fleet: app.clone()
                }
            )))
        );
        assert_eq!(fleets(&state), 1);
        assert!(app.join("fleet.toml").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_failed_delete_keeps_the_fleet_registered() {
        let (root, state) = tree("failed", &["app"]);
        let app = code(&root, "app");
        created(&state, &app, &[]);
        let _ = fs::set_permissions(&app, fs::Permissions::from_mode(0o500));
        let reply = removed(&state, &app, &[], true);
        let _ = fs::set_permissions(&app, fs::Permissions::from_mode(0o755));
        assert!(
            matches!(
                &reply,
                Some(Reply::Status(Status::File(
                    status::File::CannotWrite { .. }
                )))
            ),
            "{reply:?}"
        );
        assert_eq!(fleets(&state), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_fleet_toml_already_gone_is_missing() {
        let (root, state) = tree("missing", &["app"]);
        let app = code(&root, "app");
        created(&state, &app, &[]);
        let _ = fs::remove_file(app.join("fleet.toml"));
        let reply = removed(&state, &app, &[], true);
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::Removed {
                        config: response::ConfigFate::Missing,
                        ..
                    }
                )))
            ),
            "{reply:?}"
        );
        assert_eq!(fleets(&state), 0);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn asked_first_a_whole_remove_with_fleet_toml_changes_nothing_and_asks() {
        let (root, state) = tree("ask", &["app", "api"]);
        let app = code(&root, "app");
        created(&state, &app, &[]);
        let reply = remove(&state, &app, &[], ConfigChoice::Ask).ok();
        assert_eq!(
            reply,
            Some(answer(response::Remove::Ask {
                directory: app.clone()
            }))
        );
        assert_eq!(fleets(&state), 1);
        assert!(app.join("fleet.toml").exists());
        let _ = fs::remove_file(app.join("fleet.toml"));
        let reply = remove(&state, &app, &[], ConfigChoice::Ask).ok();
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::Removed {
                        config: response::ConfigFate::Missing,
                        ..
                    }
                )))
            ),
            "{reply:?}"
        );
        let wing = root.join("wing");
        created(&state, &wing, &[code(&root, "api"), app.clone()]);
        let reply = remove(&state, &wing, std::slice::from_ref(&app), ConfigChoice::Ask).ok();
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::ProjectsTaken { .. }
                )))
            ),
            "{reply:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_deleted_repository_written_relative_is_taken_from_file_and_registry() {
        let (root, state) = tree("relative", &["api", "blog"]);
        let wing = root.join("wing");
        created(&state, &wing, &[code(&root, "api"), code(&root, "blog")]);
        let _ = fs::write(
            wing.join("fleet.toml"),
            "projects = [\n  \"../code/api\",\n  \"../code/blog\", # gone soon\n]\n",
        );
        let _ = fs::remove_dir_all(code(&root, "blog"));
        // What the CLI sends for `--project ../code/blog` run in wing/.
        let asked = wing.join("..").join("code").join("blog");
        let reply = removed(&state, &wing, &[asked], false);
        assert!(
            matches!(
                &reply,
                Some(Reply::Response(Response::Remove(
                    response::Remove::ProjectsTaken { .. }
                )))
            ),
            "{reply:?}"
        );
        let text = fs::read_to_string(wing.join("fleet.toml")).unwrap_or_default();
        assert_eq!(text, "projects = [\n  \"../code/api\",\n]\n");
        let _ = fs::remove_dir_all(root);
    }
}
