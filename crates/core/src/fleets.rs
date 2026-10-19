//! What a fleet is, and when creating one is refused. Pure: paths arrive
//! canonical and absolute, and whether a path is a repository's root is asked
//! through a closure.

use std::path::{Path, PathBuf};

use crate::status::Status;

/// A fleet: its directory and the projects it serves, as canonical absolute
/// paths, projects sorted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fleet {
    /// The fleet directory, holding its config file.
    pub directory: PathBuf,
    /// The repositories it serves, by their roots.
    pub projects: Vec<PathBuf>,
}

impl Fleet {
    /// Whether this is a default fleet: its directory is its one project.
    #[must_use]
    pub fn is_default(&self) -> bool {
        self.projects.len() == 1 && self.projects[0] == self.directory
    }
}

/// What creating a fleet would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateDecision {
    /// Create this fleet.
    Create(Fleet),
    /// This same fleet already exists; nothing changes.
    AlreadyHere(Fleet),
    /// Refused, for this reason.
    Refuse(Status),
}

/// Decides whether a fleet in `directory` serving `projects` may be created,
/// given the fleets already `registered`. No projects means a default fleet:
/// `directory` is its own one project. `repo_root_of` gives the root of the
/// git repository containing a path, if any.
#[must_use]
pub fn decide_create(
    registered: &[Fleet],
    directory: &Path,
    projects: &[PathBuf],
    repo_root_of: impl Fn(&Path) -> Option<PathBuf>,
) -> CreateDecision {
    let projects = match resolve_projects(directory, projects, repo_root_of) {
        Ok(projects) => projects,
        Err(status) => return CreateDecision::Refuse(status),
    };
    let wanted = Fleet {
        directory: directory.to_path_buf(),
        projects,
    };
    refusal_against(registered, &wanted).unwrap_or(CreateDecision::Create(wanted))
}

/// What removing in a directory would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveDecision {
    /// Remove this whole fleet.
    Whole(Fleet),
    /// Take these projects out of this wingman fleet; it keeps the rest.
    Take {
        /// The fleet, as registered.
        fleet: Fleet,
        /// The projects to take out, sorted and once each.
        taken: Vec<PathBuf>,
    },
    /// No fleet is registered in the directory, nor around it.
    NothingHere,
    /// Refused, for this reason.
    Refuse(Status),
}

/// Decides what removing the fleet in `directory` does, given the fleets
/// `registered`. No `named` projects means the whole fleet, and so does
/// naming every one of its projects. Paths arrive canonical.
#[must_use]
pub fn decide_remove(registered: &[Fleet], directory: &Path, named: &[PathBuf]) -> RemoveDecision {
    let Some(fleet) = registered.iter().find(|f| f.directory == directory) else {
        let around = registered.iter().find(|f| {
            directory.starts_with(&f.directory)
                || f.projects.iter().any(|p| directory.starts_with(p))
        });
        return match around {
            Some(f) => RemoveDecision::Refuse(Status::NotFleetDirectory {
                directory: directory.to_path_buf(),
                fleet: f.directory.clone(),
            }),
            None => RemoveDecision::NothingHere,
        };
    };
    if let Some(stranger) = named.iter().find(|p| !fleet.projects.contains(p)) {
        return RemoveDecision::Refuse(Status::NotServed {
            path: stranger.clone(),
        });
    }
    if named.is_empty() || fleet.projects.iter().all(|p| named.contains(p)) {
        return RemoveDecision::Whole(fleet.clone());
    }
    let mut taken = named.to_vec();
    taken.sort();
    taken.dedup();
    RemoveDecision::Take {
        fleet: fleet.clone(),
        taken,
    }
}

/// The projects a fleet in `directory` serves, from the paths it was given
/// or its config names: sorted and once each. No paths means a default
/// fleet, `directory` its one project.
///
/// # Errors
///
/// Why a path cannot be a project of this fleet: not a repository's root,
/// or the fleet directory is inside it.
pub fn resolve_projects(
    directory: &Path,
    configured: &[PathBuf],
    repo_root_of: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<Vec<PathBuf>, Status> {
    if configured.is_empty() {
        return match repo_root_of(directory) {
            Some(root) if root == directory => Ok(vec![directory.to_path_buf()]),
            Some(root) => Err(Status::DirectoryNotRepoRoot {
                path: directory.to_path_buf(),
                root,
            }),
            None => Err(Status::DirectoryNotInRepo {
                path: directory.to_path_buf(),
            }),
        };
    }
    let mut sorted = configured.to_vec();
    sorted.sort();
    sorted.dedup();
    for project in &sorted {
        if let Some(refusal) = project_refusal(directory, project, repo_root_of(project)) {
            return Err(refusal);
        }
    }
    Ok(sorted)
}

/// The projects the `fleet.toml` in `directory` says the fleet serves,
/// resolved as [`resolve_projects`] does. Pure: the file is read with
/// `read`, each project path is made canonical with `canonicalize`, and
/// `repo_root_of` says where a repository's root is.
///
/// # Errors
///
/// Why the config cannot be used: the file is missing or unreadable, it
/// does not parse, a project it names does not exist, or a project is not a
/// repository's root.
pub fn read_projects(
    directory: &Path,
    read: impl Fn(&Path) -> std::io::Result<String>,
    canonicalize: impl Fn(&Path) -> std::io::Result<PathBuf>,
    repo_root_of: impl Fn(&Path) -> Option<PathBuf>,
) -> Result<Vec<PathBuf>, Status> {
    let path = directory.join(crate::paths::CONFIG_FILE);
    let text = read(&path).map_err(|e| Status::unusable(&path, &e))?;
    let parsed = crate::config::parse(&text).map_err(|e| Status::BadConfig {
        path: path.clone(),
        line: e.line,
        column: e.column,
        error: e.message,
    })?;
    let mut canonical = Vec::new();
    for project in crate::config::projects_of(&parsed, directory) {
        canonical.push(canonicalize(&project).map_err(|e| Status::unusable(&project, &e))?);
    }
    resolve_projects(directory, &canonical, repo_root_of)
}

/// A project of `wanted` that a fleet in `others` already serves.
#[must_use]
pub fn check_shared(others: &[Fleet], wanted: &Fleet) -> Option<Status> {
    others.iter().find_map(|other| {
        wanted
            .projects
            .iter()
            .find(|p| other.projects.contains(p))
            .map(|shared| Status::AlreadyServed {
                project: shared.clone(),
                fleet: other.directory.clone(),
            })
    })
}

/// Each fleet among `claims` that names a project another one also names,
/// with why. `claims` is every fleet as its config now describes it, so a
/// project moved from one fleet to another in the same edit is no conflict.
#[must_use]
pub fn conflicts(claims: &[Fleet]) -> Vec<(PathBuf, Status)> {
    claims
        .iter()
        .filter_map(|fleet| {
            claims
                .iter()
                .filter(|other| other.directory != fleet.directory)
                .find_map(|other| {
                    fleet
                        .projects
                        .iter()
                        .find(|p| other.projects.contains(p))
                        .map(|shared| Status::AlsoClaimed {
                            project: shared.clone(),
                            fleet: other.directory.clone(),
                        })
                })
                .map(|status| (fleet.directory.clone(), status))
        })
        .collect()
}

/// What re-reading a fleet's config changes in its projects.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Change {
    /// Projects the config names that the fleet did not serve.
    pub added: Vec<PathBuf>,
    /// Projects the fleet served that the config no longer names.
    pub removed: Vec<PathBuf>,
}

/// The change from the `current` projects to the `wanted` ones.
#[must_use]
pub fn sync(current: &[PathBuf], wanted: &[PathBuf]) -> Change {
    Change {
        added: wanted
            .iter()
            .filter(|p| !current.contains(p))
            .cloned()
            .collect(),
        removed: current
            .iter()
            .filter(|p| !wanted.contains(p))
            .cloned()
            .collect(),
    }
}

fn project_refusal(directory: &Path, project: &Path, root: Option<PathBuf>) -> Option<Status> {
    match root {
        None => {
            return Some(Status::ProjectNotInRepo {
                path: project.to_path_buf(),
            });
        }
        Some(root) if root != project => {
            return Some(Status::ProjectNotRepoRoot {
                path: project.to_path_buf(),
                root,
            });
        }
        Some(_) => {}
    }
    if directory == project {
        return Some(Status::ProjectIsFleetDirectory {
            path: project.to_path_buf(),
        });
    }
    if directory.starts_with(project) {
        return Some(Status::WingmanInsideProject {
            directory: directory.to_path_buf(),
            project: project.to_path_buf(),
        });
    }
    None
}

fn sorted(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut paths = paths.to_vec();
    paths.sort();
    paths
}

fn refusal_against(registered: &[Fleet], wanted: &Fleet) -> Option<CreateDecision> {
    let refuse = |r| Some(CreateDecision::Refuse(r));
    if let Some(same) = registered.iter().find(|f| f.directory == wanted.directory) {
        if sorted(&same.projects) == sorted(&wanted.projects) {
            return Some(CreateDecision::AlreadyHere(same.clone()));
        }
        return refuse(Status::AlreadyHereServingOthers {
            directory: wanted.directory.clone(),
            serving: same.projects.clone(),
            asked: wanted.projects.clone(),
        });
    }
    for other in registered {
        if wanted.directory.starts_with(&other.directory) {
            return refuse(Status::InsideFleet {
                directory: wanted.directory.clone(),
                fleet: other.directory.clone(),
            });
        }
        if other.directory.starts_with(&wanted.directory) {
            return refuse(Status::ContainsFleet {
                directory: wanted.directory.clone(),
                fleet: other.directory.clone(),
            });
        }
        let inside = other
            .projects
            .iter()
            .find(|p| wanted.directory.starts_with(p) && wanted.directory != **p);
        if let Some(project) = inside {
            return refuse(Status::InsideServedProject {
                directory: wanted.directory.clone(),
                project: project.clone(),
                fleet: other.directory.clone(),
            });
        }
    }
    check_shared(registered, wanted).map(CreateDecision::Refuse)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    /// The root of the repository containing a path, among `roots`.
    fn repos(roots: &'static [&'static str]) -> impl Fn(&Path) -> Option<PathBuf> {
        move |path| {
            roots
                .iter()
                .map(Path::new)
                .filter(|r| path.starts_with(r))
                .max_by_key(|r| r.as_os_str().len())
                .map(Path::to_path_buf)
        }
    }

    fn fleet(dir: &str, projects: &[&str]) -> Fleet {
        Fleet {
            directory: p(dir),
            projects: projects.iter().map(|s| p(s)).collect(),
        }
    }

    #[test]
    fn sync_names_what_was_added_and_removed() {
        let change = sync(&[p("/c/api"), p("/c/app")], &[p("/c/app"), p("/c/web")]);
        assert_eq!(
            change,
            Change {
                added: vec![p("/c/web")],
                removed: vec![p("/c/api")]
            }
        );
        assert_eq!(sync(&[p("/c/a")], &[p("/c/a")]), Change::default());
    }

    #[test]
    fn a_project_moved_between_fleets_is_no_conflict() {
        let claims = [
            fleet("/f/a", &["/c/api", "/c/web"]),
            fleet("/f/w", &["/c/app"]),
        ];
        assert_eq!(conflicts(&claims), Vec::new());
    }

    #[test]
    fn two_fleets_claiming_one_project_are_both_named() {
        let claims = [
            fleet("/f/a", &["/c/api"]),
            fleet("/f/w", &["/c/api", "/c/app"]),
        ];
        assert_eq!(
            conflicts(&claims),
            vec![
                (
                    p("/f/a"),
                    Status::AlsoClaimed {
                        project: p("/c/api"),
                        fleet: p("/f/w")
                    }
                ),
                (
                    p("/f/w"),
                    Status::AlsoClaimed {
                        project: p("/c/api"),
                        fleet: p("/f/a")
                    }
                ),
            ]
        );
    }

    #[test]
    fn check_shared_names_the_other_fleet() {
        let others = [fleet("/f/one", &["/c/api"])];
        let wanted = fleet("/f/two", &["/c/app", "/c/api"]);
        assert_eq!(
            check_shared(&others, &wanted),
            Some(Status::AlreadyServed {
                project: p("/c/api"),
                fleet: p("/f/one")
            })
        );
        assert_eq!(check_shared(&others, &fleet("/f/two", &["/c/app"])), None);
    }

    #[test]
    fn resolve_projects_gives_a_default_fleet_its_directory() {
        assert_eq!(
            resolve_projects(&p("/c/a"), &[], repos(&["/c/a"])),
            Ok(vec![p("/c/a")])
        );
        assert!(resolve_projects(&p("/f/w"), &[p("/c/x")], repos(&[])).is_err());
    }

    #[test]
    fn a_default_fleet_is_its_own_one_project() {
        let d = decide_create(&[], &p("/c/blog"), &[], repos(&["/c/blog"]));
        assert_eq!(d, CreateDecision::Create(fleet("/c/blog", &["/c/blog"])));
    }

    #[test]
    fn a_default_fleet_outside_a_repo_root_is_refused() {
        let d = decide_create(&[], &p("/c/blog/src"), &[], repos(&["/c/blog"]));
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::DirectoryNotRepoRoot {
                path: p("/c/blog/src"),
                root: p("/c/blog")
            })
        );
        let d = decide_create(&[], &p("/c/notes"), &[], repos(&[]));
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::DirectoryNotInRepo {
                path: p("/c/notes")
            })
        );
    }

    #[test]
    fn a_wingman_fleet_serves_its_projects_sorted_and_once() {
        let d = decide_create(
            &[],
            &p("/f/work"),
            &[p("/c/app"), p("/c/api"), p("/c/app")],
            repos(&["/c/app", "/c/api"]),
        );
        assert_eq!(
            d,
            CreateDecision::Create(fleet("/f/work", &["/c/api", "/c/app"]))
        );
    }

    #[test]
    fn a_wingman_project_that_is_not_a_repo_root_is_refused() {
        let d = decide_create(&[], &p("/f/work"), &[p("/c/notes")], repos(&[]));
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::ProjectNotInRepo {
                path: p("/c/notes")
            })
        );
    }

    #[test]
    fn a_wingman_fleet_inside_its_own_project_is_refused() {
        let d = decide_create(&[], &p("/c/app/.fleet"), &[p("/c/app")], repos(&["/c/app"]));
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::WingmanInsideProject {
                directory: p("/c/app/.fleet"),
                project: p("/c/app")
            })
        );
    }

    #[test]
    fn the_same_fleet_again_is_already_here() {
        let existing = [fleet("/c/blog", &["/c/blog"])];
        let d = decide_create(&existing, &p("/c/blog"), &[], repos(&["/c/blog"]));
        assert_eq!(
            d,
            CreateDecision::AlreadyHere(fleet("/c/blog", &["/c/blog"]))
        );
    }

    #[test]
    fn the_same_directory_with_other_projects_is_refused() {
        let existing = [fleet("/f/work", &["/c/app"])];
        let d = decide_create(
            &existing,
            &p("/f/work"),
            &[p("/c/api")],
            repos(&["/c/app", "/c/api"]),
        );
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::AlreadyHereServingOthers {
                directory: p("/f/work"),
                serving: vec![p("/c/app")],
                asked: vec![p("/c/api")],
            })
        );
    }

    #[test]
    fn a_fleet_above_or_below_another_is_refused() {
        let existing = [fleet("/f/work", &["/c/app"])];
        let below = decide_create(
            &existing,
            &p("/f/work/sub"),
            &[p("/c/api")],
            repos(&["/c/api"]),
        );
        assert_eq!(
            below,
            CreateDecision::Refuse(Status::InsideFleet {
                directory: p("/f/work/sub"),
                fleet: p("/f/work")
            })
        );
        let above = decide_create(&existing, &p("/f"), &[p("/c/api")], repos(&["/c/api"]));
        assert_eq!(
            above,
            CreateDecision::Refuse(Status::ContainsFleet {
                directory: p("/f"),
                fleet: p("/f/work")
            })
        );
    }

    #[test]
    fn nesting_is_named_before_a_shared_project() {
        let existing = [
            fleet("/c/blog", &["/c/blog"]),
            fleet("/f/work", &["/c/app"]),
        ];
        let d = decide_create(
            &existing,
            &p("/f/work/sub"),
            &[p("/c/blog")],
            repos(&["/c/blog"]),
        );
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::InsideFleet {
                directory: p("/f/work/sub"),
                fleet: p("/f/work")
            })
        );
    }

    #[test]
    fn the_same_wingman_fleet_again_is_already_here_whatever_order_state_returns() {
        // SQL sorts "/c/acme-legacy/site" before "/c/acme/api"; Rust paths do not.
        let existing = [fleet("/f/work", &["/c/acme-legacy/site", "/c/acme/api"])];
        let d = decide_create(
            &existing,
            &p("/f/work"),
            &[p("/c/acme/api"), p("/c/acme-legacy/site")],
            repos(&["/c/acme/api", "/c/acme-legacy/site"]),
        );
        assert!(matches!(d, CreateDecision::AlreadyHere(_)), "{d:?}");
    }

    #[test]
    fn a_fleet_inside_another_fleets_project_is_refused() {
        let existing = [fleet("/f/wing", &["/c/api"])];
        let d = decide_create(
            &existing,
            &p("/c/api/tools"),
            &[p("/c/app")],
            repos(&["/c/api", "/c/app"]),
        );
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::InsideServedProject {
                directory: p("/c/api/tools"),
                project: p("/c/api"),
                fleet: p("/f/wing"),
            })
        );
    }

    #[test]
    fn a_project_that_is_the_fleet_directory_points_at_the_default_fleet() {
        let d = decide_create(&[], &p("/c/app"), &[p("/c/app")], repos(&["/c/app"]));
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::ProjectIsFleetDirectory { path: p("/c/app") })
        );
    }

    #[test]
    fn a_project_that_is_not_a_root_names_its_root() {
        let d = decide_create(&[], &p("/f/work"), &[p("/c/app/src")], repos(&["/c/app"]));
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::ProjectNotRepoRoot {
                path: p("/c/app/src"),
                root: p("/c/app")
            })
        );
    }

    #[test]
    fn a_project_another_fleet_serves_is_refused() {
        let existing = [fleet("/f/work", &["/c/app"])];
        let d = decide_create(&existing, &p("/c/app"), &[], repos(&["/c/app"]));
        assert_eq!(
            d,
            CreateDecision::Refuse(Status::AlreadyServed {
                project: p("/c/app"),
                fleet: p("/f/work")
            })
        );
    }

    #[test]
    fn plain_remove_takes_the_whole_fleet_here() {
        let registered = [fleet("/w", &["/c/api", "/c/blog"])];
        assert_eq!(
            decide_remove(&registered, Path::new("/w"), &[]),
            RemoveDecision::Whole(registered[0].clone())
        );
    }

    #[test]
    fn naming_some_projects_takes_them_and_naming_all_takes_the_fleet() {
        let registered = [fleet("/w", &["/c/api", "/c/blog"])];
        assert_eq!(
            decide_remove(&registered, Path::new("/w"), &[p("/c/blog")]),
            RemoveDecision::Take {
                fleet: registered[0].clone(),
                taken: vec![p("/c/blog")],
            }
        );
        assert_eq!(
            decide_remove(&registered, Path::new("/w"), &[p("/c/blog"), p("/c/api")]),
            RemoveDecision::Whole(registered[0].clone())
        );
        let default = [fleet("/c/app", &["/c/app"])];
        assert_eq!(
            decide_remove(&default, Path::new("/c/app"), &[p("/c/app")]),
            RemoveDecision::Whole(default[0].clone())
        );
    }

    #[test]
    fn a_project_this_fleet_does_not_serve_is_refused() {
        let registered = [fleet("/w", &["/c/api"]), fleet("/c/app", &["/c/app"])];
        assert_eq!(
            decide_remove(&registered, Path::new("/w"), &[p("/c/app")]),
            RemoveDecision::Refuse(Status::NotServed { path: p("/c/app") })
        );
    }

    #[test]
    fn inside_a_fleet_or_its_project_names_the_fleet_and_elsewhere_is_nothing_here() {
        let registered = [fleet("/c/app", &["/c/app"]), fleet("/w", &["/c/api"])];
        assert_eq!(
            decide_remove(&registered, Path::new("/c/app/src"), &[]),
            RemoveDecision::Refuse(Status::NotFleetDirectory {
                directory: p("/c/app/src"),
                fleet: p("/c/app"),
            })
        );
        assert_eq!(
            decide_remove(&registered, Path::new("/c/api/lib"), &[]),
            RemoveDecision::Refuse(Status::NotFleetDirectory {
                directory: p("/c/api/lib"),
                fleet: p("/w"),
            })
        );
        assert_eq!(
            decide_remove(&registered, Path::new("/elsewhere"), &[]),
            RemoveDecision::NothingHere
        );
    }
}
