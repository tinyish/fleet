//! What create, remove, start and stop change: fleets registered from
//! their `fleet.toml`, adopted or written, removed or cut down, and each
//! fleet's config re-read, its projects and roster with it.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use fleet_core::protocol::request::ConfigChoice;
use fleet_core::protocol::{Reply, Request, Response, Status, request, response};
use fleet_core::status;

use super::{Fakes, Rig, bare, rig};
use crate::testing::{code, steps};

fn created(reply: &Reply) -> Option<&response::Create> {
    match reply {
        Reply::Response(Response::Create(c)) => Some(c),
        _ => None,
    }
}

/// The fleets status lists from outside every fleet.
fn fleets(r: &Rig) -> Vec<PathBuf> {
    r.status(&r.root)
        .fleets
        .into_iter()
        .map(|f| f.directory)
        .collect()
}

/// The seats of the fleet in `fleet`, as status shows them.
fn seats_of(r: &Rig, fleet: &Path) -> Vec<String> {
    r.status(fleet)
        .fleets
        .into_iter()
        .flat_map(|f| f.seats)
        .map(|s| s.name)
        .collect()
}

fn config_fate(reply: &Reply) -> Option<response::ConfigFate> {
    match reply {
        Reply::Response(Response::Remove(response::Remove::Removed { config, .. })) => {
            Some(*config)
        }
        _ => None,
    }
}

#[test]
fn a_default_fleet_toml_left_behind_is_adopted_as_it_is() {
    let r = bare("adopt-default", Fakes::default());
    let text = "# kept by fleet remove\n";
    assert!(fs::write(r.app.join("fleet.toml"), text).is_ok());
    let reply = r.create(&r.app, &[]);
    let made = created(&reply);
    assert_eq!(made.map(|c| (c.created, c.adopted)), Some((true, true)));
    assert_eq!(made.map(|c| c.projects.clone()), Some(vec![r.app.clone()]));
    assert_eq!(
        fs::read_to_string(r.app.join("fleet.toml")).ok().as_deref(),
        Some(text)
    );
    assert_eq!(fleets(&r), std::slice::from_ref(&r.app));
}

#[test]
fn a_hand_written_wingman_file_is_adopted_and_a_matching_project_list_too() {
    let r = bare("adopt-wing", Fakes::default());
    let wing = r.root.join("wing");
    let text = "# mine\nprojects = [\"../code/blog\", # blog\n  \"../code/api\"]\n";
    assert!(fs::write(wing.join("fleet.toml"), text).is_ok());
    let asked = [code(&r.root, "api"), wing.join("../code/blog")];
    let reply = r.create(&wing, &asked);
    let made = created(&reply);
    assert_eq!(made.map(|c| c.adopted), Some(true));
    assert_eq!(
        made.map(|c| c.projects.clone()),
        Some(vec![code(&r.root, "api"), code(&r.root, "blog")])
    );
    assert_eq!(
        fs::read_to_string(wing.join("fleet.toml")).ok().as_deref(),
        Some(text)
    );
}

#[test]
fn a_project_list_that_differs_from_the_file_is_refused_and_changes_nothing() {
    let r = bare("adopt-differs", Fakes::default());
    let wing = r.root.join("wing");
    let text = "projects = [\"../code/api\", \"../code/blog\"]\n";
    assert!(fs::write(wing.join("fleet.toml"), text).is_ok());
    assert_eq!(
        r.create(&wing, &[code(&r.root, "api")]),
        Reply::Status(Status::File(status::File::ConfigDiffers {
            path: wing.join("fleet.toml"),
            listed: vec![code(&r.root, "api"), code(&r.root, "blog")],
        }))
    );
    assert_eq!(fleets(&r), Vec::<PathBuf>::new());
}

#[test]
fn a_file_that_does_not_read_is_refused_for_what_is_wrong() {
    let r = bare("adopt-broken", Fakes::default());
    assert!(fs::write(r.app.join("fleet.toml"), "projects = [").is_ok());
    let reply = r.create(&r.app, &[]);
    assert!(
        matches!(
            reply,
            Reply::Status(Status::File(status::File::BadConfig { .. }))
        ),
        "{reply:?}"
    );
    assert_eq!(fleets(&r), Vec::<PathBuf>::new());
}

#[test]
fn a_file_listing_a_project_another_fleet_serves_is_refused() {
    let r = bare("adopt-served", Fakes::default());
    let api = code(&r.root, "api");
    assert!(created(&r.create(&api, &[])).is_some());
    let wing = r.root.join("wing");
    assert!(fs::write(wing.join("fleet.toml"), "projects = [\"../code/api\"]\n").is_ok());
    let reply = r.create(&wing, &[]);
    assert!(
        matches!(
            reply,
            Reply::Status(Status::Fleet(status::Fleet::AlreadyServed { .. }))
        ),
        "{reply:?}"
    );
    assert_eq!(fleets(&r), [api]);
}

#[test]
fn a_new_fleet_writes_its_file_has_no_seats_and_is_not_adopted() {
    let r = bare("create-fresh", Fakes::default());
    let reply = r.create(&r.app, &[]);
    assert_eq!(created(&reply).map(|c| c.adopted), Some(false));
    assert!(r.app.join("fleet.toml").exists());
    assert_eq!(seats_of(&r, &r.app), Vec::<String>::new());
}

#[test]
fn an_adopted_file_brings_its_roles_and_seats() {
    let r = bare("adopt-seats", Fakes::default());
    let text = "[roles.w]\nadapter = \"/a/claude\"\n[seats.scribe]\nrole = \"w\"\n";
    assert!(fs::write(r.app.join("fleet.toml"), text).is_ok());
    assert!(created(&r.create(&r.app, &[])).is_some());
    assert_eq!(seats_of(&r, &r.app), ["scribe"]);
}

#[test]
fn an_adopted_file_whose_seats_break_a_rule_is_refused() {
    let r = bare("adopt-bad-seat", Fakes::default());
    assert!(fs::write(r.app.join("fleet.toml"), "[seats.scribe]\nrole = \"w\"\n").is_ok());
    let reply = r.create(&r.app, &[]);
    assert!(
        matches!(
            reply,
            Reply::Status(Status::File(status::File::UnknownRole { .. }))
        ),
        "{reply:?}"
    );
    assert_eq!(fleets(&r), Vec::<PathBuf>::new());
}

#[test]
fn a_whole_remove_keeps_or_deletes_fleet_toml() {
    let r = bare("remove-whole", Fakes::default());
    let config = r.app.join("fleet.toml");
    assert!(created(&r.create(&r.app, &[])).is_some());
    let reply = r.remove(&r.app, &[], ConfigChoice::Keep);
    assert_eq!(
        config_fate(&reply),
        Some(response::ConfigFate::Kept),
        "{reply:?}"
    );
    assert!(config.exists());
    assert_eq!(fleets(&r), Vec::<PathBuf>::new());
    assert!(fs::remove_file(&config).is_ok());
    assert!(created(&r.create(&r.app, &[])).is_some());
    let reply = r.remove(&r.app, &[], ConfigChoice::Delete);
    assert_eq!(
        config_fate(&reply),
        Some(response::ConfigFate::Deleted),
        "{reply:?}"
    );
    assert!(!config.exists());
    assert_eq!(fleets(&r), Vec::<PathBuf>::new());
}

#[test]
fn removing_twice_is_nothing_here() {
    let r = rig("remove-twice");
    let _ = r.remove(&r.app, &[], ConfigChoice::Keep);
    let reply = r.remove(&r.app, &[], ConfigChoice::Delete);
    assert!(
        matches!(
            reply,
            Reply::Response(Response::Remove(response::Remove::NothingHere { .. }))
        ),
        "{reply:?}"
    );
    assert!(r.app.join("fleet.toml").exists());
}

/// A wingman fleet in `wing/` serving `projects` of `code/`, made by a
/// create request: `wing/`.
fn wingman(r: &Rig, projects: &[&str]) -> PathBuf {
    let wing = r.root.join("wing");
    let projects: Vec<PathBuf> = projects.iter().map(|p| code(&r.root, p)).collect();
    let reply = r.create(&wing, &projects);
    assert!(created(&reply).is_some(), "{reply:?}");
    wing
}

/// `stop --project` on `project`, from anywhere.
fn stop_project(r: &Rig, project: PathBuf) -> Reply {
    r.ask(Request::Stop(request::Stop::Project { project }))
}

#[test]
fn a_take_edits_fleet_toml_and_keeps_the_fleet() {
    let r = bare("remove-take", Fakes::default());
    let wing = wingman(&r, &["api", "blog"]);
    let (api, blog) = (code(&r.root, "api"), code(&r.root, "blog"));
    assert_eq!(
        r.remove(&wing, std::slice::from_ref(&blog), ConfigChoice::Delete),
        Reply::Response(Response::Remove(response::Remove::ProjectsTaken {
            directory: wing.clone(),
            taken: vec![blog.clone()],
            remaining: vec![api.clone()],
        }))
    );
    let text = fs::read_to_string(wing.join("fleet.toml")).unwrap_or_default();
    assert!(!text.contains("blog") && text.contains("api"), "{text}");
    assert_eq!(fleets(&r), [wing]);
    assert!(matches!(
        stop_project(&r, blog),
        Reply::Status(Status::Project(status::Project::NotServed { .. }))
    ));
    assert!(matches!(stop_project(&r, api), Reply::Response(_)));
}

#[test]
fn taking_the_last_project_removes_the_fleet() {
    let r = bare("remove-last", Fakes::default());
    let wing = wingman(&r, &["api"]);
    let reply = r.remove(&wing, &[code(&r.root, "api")], ConfigChoice::Keep);
    assert_eq!(
        config_fate(&reply),
        Some(response::ConfigFate::Kept),
        "{reply:?}"
    );
    assert_eq!(fleets(&r), Vec::<PathBuf>::new());
}

#[test]
fn inside_a_fleet_is_refused_and_changes_nothing() {
    let r = rig("remove-inside");
    let src = r.app.join("src");
    assert!(fs::create_dir_all(&src).is_ok());
    assert_eq!(
        r.remove(&src, &[], ConfigChoice::Delete),
        Reply::Status(Status::Fleet(status::Fleet::NotFleetDirectory {
            directory: src,
            fleet: r.app.clone()
        }))
    );
    assert_eq!(fleets(&r), std::slice::from_ref(&r.app));
    assert!(r.app.join("fleet.toml").exists());
}

#[test]
fn a_failed_delete_keeps_the_fleet_registered() {
    let r = rig("remove-failed");
    let _ = fs::set_permissions(&r.app, fs::Permissions::from_mode(0o500));
    let reply = r.remove(&r.app, &[], ConfigChoice::Delete);
    let _ = fs::set_permissions(&r.app, fs::Permissions::from_mode(0o755));
    assert!(
        matches!(
            reply,
            Reply::Status(Status::File(status::File::CannotWrite { .. }))
        ),
        "{reply:?}"
    );
    assert_eq!(fleets(&r), std::slice::from_ref(&r.app));
}

#[test]
fn a_fleet_toml_already_gone_is_missing() {
    let r = rig("remove-missing");
    assert!(fs::remove_file(r.app.join("fleet.toml")).is_ok());
    let reply = r.remove(&r.app, &[], ConfigChoice::Delete);
    assert_eq!(
        config_fate(&reply),
        Some(response::ConfigFate::Missing),
        "{reply:?}"
    );
    assert_eq!(fleets(&r), Vec::<PathBuf>::new());
}

#[test]
fn asked_first_a_whole_remove_with_fleet_toml_changes_nothing_and_asks() {
    let r = rig("remove-ask");
    assert_eq!(
        r.remove(&r.app, &[], ConfigChoice::Ask),
        Reply::Response(Response::Remove(response::Remove::Ask {
            directory: r.app.clone()
        }))
    );
    assert_eq!(fleets(&r), std::slice::from_ref(&r.app));
    assert!(r.app.join("fleet.toml").exists());
    assert!(fs::remove_file(r.app.join("fleet.toml")).is_ok());
    let reply = r.remove(&r.app, &[], ConfigChoice::Ask);
    assert_eq!(
        config_fate(&reply),
        Some(response::ConfigFate::Missing),
        "{reply:?}"
    );
    let wing = wingman(&r, &["api", "app"]);
    let reply = r.remove(&wing, std::slice::from_ref(&r.app), ConfigChoice::Ask);
    assert!(
        matches!(
            reply,
            Reply::Response(Response::Remove(response::Remove::ProjectsTaken { .. }))
        ),
        "{reply:?}"
    );
}

#[test]
fn a_deleted_repository_written_relative_is_taken_from_file_and_registry() {
    let r = bare("remove-relative", Fakes::default());
    let wing = wingman(&r, &["api", "blog"]);
    let text = "projects = [\n  \"../code/api\",\n  \"../code/blog\", # gone soon\n]\n";
    assert!(fs::write(wing.join("fleet.toml"), text).is_ok());
    assert!(fs::remove_dir_all(code(&r.root, "blog")).is_ok());
    // What the CLI sends for `--project ../code/blog` run in wing/.
    let asked = wing.join("..").join("code").join("blog");
    let reply = r.remove(&wing, &[asked], ConfigChoice::Keep);
    assert!(
        matches!(
            reply,
            Reply::Response(Response::Remove(response::Remove::ProjectsTaken { .. }))
        ),
        "{reply:?}"
    );
    let text = fs::read_to_string(wing.join("fleet.toml")).unwrap_or_default();
    assert_eq!(text, "projects = [\n  \"../code/api\",\n]\n");
}

/// Writes `wing/fleet.toml` listing `projects` of `code/` by their full
/// paths, then `extra`.
fn write_config(r: &Rig, projects: &[&str], extra: &str) {
    let list: Vec<String> = projects
        .iter()
        .map(|p| format!("{:?}", code(&r.root, p).display().to_string()))
        .collect();
    let text = format!("projects = [{}]\n{extra}", list.join(", "));
    assert!(fs::write(r.root.join("wing").join("fleet.toml"), text).is_ok());
}

fn all_on(r: &Rig) -> Option<response::AllOn> {
    match r.start_all() {
        Reply::Response(Response::Start(response::Start::AllOn(all))) => Some(all),
        _ => None,
    }
}

#[test]
fn a_refused_start_project_changes_nothing() {
    let r = bare("start-refused", Fakes::default());
    let _ = wingman(&r, &["app"]);
    write_config(&r, &["app", "lib"], "");
    let before = steps(&r.sup);
    let reply = r.ask(Request::Start(request::Start::Project {
        project: code(&r.root, "blog"),
    }));
    assert!(
        matches!(
            reply,
            Reply::Status(Status::Project(status::Project::NotServed { .. }))
        ),
        "{reply:?}"
    );
    assert_eq!(steps(&r.sup), before, "lib not picked up");
}

#[test]
fn a_fleet_whose_directory_is_gone_is_forgotten_by_start() {
    let r = rig("start-gone");
    let wing = wingman(&r, &["api"]);
    assert!(fs::remove_dir_all(&wing).is_ok());
    let all = all_on(&r);
    assert_eq!(all.as_ref().map(|a| a.forgotten.clone()), Some(vec![wing]));
    assert_eq!(all.map(|a| a.stayed_off.len()), Some(0));
    assert_eq!(fleets(&r), std::slice::from_ref(&r.app));
}

#[test]
fn a_directory_that_cannot_be_looked_at_is_not_forgotten() {
    if std::env::var("USER").is_ok_and(|u| u == "root") {
        return;
    }
    let r = bare("start-locked", Fakes::default());
    let locked = r.root.join("locked");
    let fleet = locked.join("wing");
    assert!(fs::create_dir_all(&fleet).is_ok());
    assert!(created(&r.create(&fleet, &[code(&r.root, "api")])).is_some());
    let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o000));
    let all = all_on(&r);
    let _ = fs::set_permissions(&locked, fs::Permissions::from_mode(0o755));
    assert_eq!(all.as_ref().map(|a| a.forgotten.len()), Some(0), "{all:?}");
    assert_eq!(all.map(|a| a.stayed_off.len()), Some(1));
    assert_eq!(fleets(&r), [fleet]);
}

/// A role `w` and a seat `scribe` of it.
const SEAT: &str = "[roles.w]\nadapter = \"/a/claude\"\n[seats.scribe]\nrole = \"w\"\n";

#[test]
fn start_keeps_each_fleets_roster() {
    let r = bare("start-roster", Fakes::default());
    let wing = wingman(&r, &["api"]);
    write_config(&r, &["api"], SEAT);
    assert!(all_on(&r).is_some());
    assert_eq!(seats_of(&r, &wing), ["scribe"]);
    write_config(
        &r,
        &["api"],
        "[roles.w]\nadapter = \"/a/other\"\n[seats.critic]\nrole = \"w\"\n",
    );
    assert!(all_on(&r).is_some());
    assert_eq!(seats_of(&r, &wing), ["critic"]);
}

#[test]
fn a_broken_config_keeps_the_last_roster() {
    let r = bare("start-broken", Fakes::default());
    let wing = wingman(&r, &["api"]);
    write_config(&r, &["api"], SEAT);
    assert!(all_on(&r).is_some());
    write_config(&r, &["api"], "[seats.scribe]\nrole = \"nobody\"\n");
    assert_eq!(all_on(&r).map(|a| a.stayed_off.len()), Some(1));
    assert_eq!(
        seats_of(&r, &wing),
        ["scribe"],
        "the snapshot stays as it was"
    );
}

#[test]
fn start_project_keeps_the_roster_too() {
    let r = bare("start-project", Fakes::default());
    let wing = wingman(&r, &["api"]);
    write_config(&r, &["api"], SEAT);
    let reply = r.ask(Request::Start(request::Start::Project {
        project: code(&r.root, "api"),
    }));
    assert!(matches!(reply, Reply::Response(_)), "{reply:?}");
    assert_eq!(seats_of(&r, &wing), ["scribe"]);
}
