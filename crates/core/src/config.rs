//! A fleet's config file, `fleet.toml`: what the person writes and the
//! supervisor reads. Pure: rendering and parsing text.

use serde::{Deserialize, Serialize};

use crate::fleets::Fleet;

/// A fleet's config file. An empty `projects` means the fleet directory is
/// its own one project, so a default fleet's file has no `projects` key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// The repositories a wingman fleet serves, as absolute paths.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<String>,
}

/// The config file could not be read: where, when the parser knows, and
/// what it said. Facts: the CLI words them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError {
    /// The line, counting from 1.
    pub line: Option<usize>,
    /// The column on that line, in characters, counting from 1.
    pub column: Option<usize>,
    /// What the parser said, in one line.
    pub message: String,
}

const HEADER: &str = "# A tinyfleet fleet. With no `projects`, this directory is the fleet's one\n\
# project; a wingman fleet lists the repositories it serves.\n";

impl Config {
    /// The config a newly created `fleet` starts with.
    #[must_use]
    pub fn for_fleet(fleet: &Fleet) -> Config {
        if fleet.is_default() {
            return Config::default();
        }
        Config {
            projects: fleet
                .projects
                .iter()
                .map(|p| p.display().to_string())
                .collect(),
        }
    }
}

/// The projects `config` names, with paths relative to the fleet
/// `directory` resolved against it. Empty for a default fleet.
#[must_use]
pub fn projects_of(config: &Config, directory: &std::path::Path) -> Vec<std::path::PathBuf> {
    config.projects.iter().map(|p| directory.join(p)).collect()
}

/// The text of `fleet.toml` for `config`.
#[must_use]
pub fn render(config: &Config) -> String {
    let body = toml::to_string(config).unwrap_or_default();
    if body.trim().is_empty() {
        HEADER.to_owned()
    } else {
        format!("{HEADER}\n{body}")
    }
}

/// Reads `fleet.toml`.
///
/// # Errors
///
/// When the text is not TOML or holds a key this version does not know.
pub fn parse(text: &str) -> Result<Config, ConfigError> {
    toml::from_str(text).map_err(|e| {
        let at = e.span().map(|span| {
            let before = &text[..span.start.min(text.len())];
            let line = before.matches('\n').count() + 1;
            let column = before
                .rsplit('\n')
                .next()
                .unwrap_or_default()
                .chars()
                .count()
                + 1;
            (line, column)
        });
        ConfigError {
            line: at.map(|(line, _)| line),
            column: at.map(|(_, column)| column),
            message: e.message().trim().replace('\n', " "),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn relative_projects_resolve_against_the_fleet_directory() {
        let config = Config {
            projects: vec!["../code/app".into(), "/c/api".into()],
        };
        assert_eq!(
            projects_of(&config, std::path::Path::new("/f/work")),
            vec![
                PathBuf::from("/f/work/../code/app"),
                PathBuf::from("/c/api")
            ]
        );
        assert_eq!(
            projects_of(&Config::default(), std::path::Path::new("/c/blog")),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn a_default_fleet_has_no_projects_key() {
        let fleet = Fleet {
            directory: PathBuf::from("/c/blog"),
            projects: vec![PathBuf::from("/c/blog")],
        };
        let text = render(&Config::for_fleet(&fleet));
        assert!(!text.lines().any(|l| l.starts_with("projects")), "{text}");
        assert_eq!(
            parse(&text).ok(),
            Some(Config {
                projects: Vec::new()
            })
        );
    }

    #[test]
    fn a_wingman_fleet_lists_absolute_projects_and_round_trips() {
        let fleet = Fleet {
            directory: PathBuf::from("/f/work"),
            projects: vec![PathBuf::from("/c/api"), PathBuf::from("/c/my \"app\"")],
        };
        let config = Config::for_fleet(&fleet);
        let text = render(&config);
        assert!(text.contains("projects = ["), "{text}");
        assert_eq!(parse(&text).ok(), Some(config));
    }

    #[test]
    fn a_parse_error_says_where_as_facts_and_what_in_one_line() {
        let e = parse("projects = [").err();
        assert_eq!(e.as_ref().and_then(|e| e.line), Some(1));
        assert_eq!(e.as_ref().and_then(|e| e.column), Some(13));
        let message = e.map(|e| e.message).unwrap_or_default();
        assert!(
            !message.is_empty() && !message.contains('\n'),
            "{message:?}"
        );
        assert!(!message.contains("line 1"), "{message:?}");
    }

    #[test]
    fn broken_toml_and_unknown_keys_are_errors() {
        assert!(parse("projects = [").is_err());
        assert!(parse("project = []").is_err());
    }
}
