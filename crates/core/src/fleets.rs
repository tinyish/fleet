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
    let projects = if projects.is_empty() {
        match repo_root_of(directory) {
            Some(root) if root == directory => {}
            Some(root) => {
                return CreateDecision::Refuse(Status::DirectoryNotRepoRoot {
                    path: directory.to_path_buf(),
                    root,
                });
            }
            None => {
                return CreateDecision::Refuse(Status::DirectoryNotInRepo {
                    path: directory.to_path_buf(),
                });
            }
        }
        vec![directory.to_path_buf()]
    } else {
        let mut sorted = projects.to_vec();
        sorted.sort();
        sorted.dedup();
        for project in &sorted {
            if let Some(refusal) = project_refusal(directory, project, repo_root_of(project)) {
                return CreateDecision::Refuse(refusal);
            }
        }
        sorted
    };
    let wanted = Fleet {
        directory: directory.to_path_buf(),
        projects,
    };
    refusal_against(registered, &wanted).unwrap_or(CreateDecision::Create(wanted))
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
    for other in registered {
        if let Some(shared) = wanted.projects.iter().find(|p| other.projects.contains(p)) {
            return refuse(Status::AlreadyServed {
                project: shared.clone(),
                fleet: other.directory.clone(),
            });
        }
    }
    None
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
}
