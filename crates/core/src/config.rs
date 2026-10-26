//! A fleet's config file, `fleet.toml`: what the person writes and the
//! supervisor reads. Pure: rendering and parsing text.

use std::collections::BTreeMap;

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
    /// How to run each kind of agent, by its label.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub roles: BTreeMap<String, RoleEntry>,
    /// The agents the supervisor keeps running, by their names.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub seats: BTreeMap<String, SeatEntry>,
}

/// One `[roles.<label>]` table, as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoleEntry {
    /// The agent's adapter: a path, or `<name>@<version>`.
    pub adapter: String,
    /// The model, in the agent's own naming.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Where the agent works, relative to the fleet directory or absolute.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    /// What the agent is woken with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

/// One `[seats.<name>]` table, as written.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeatEntry {
    /// The label of the role it runs as.
    pub role: String,
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
            ..Config::default()
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

/// `text` with the `projects` entries for which `drop` is true taken out,
/// and everything else as the person wrote it: comments, spacing, relative
/// paths. A comment ending an entry's line goes with that entry; the one
/// ending the line before stays where it is.
///
/// # Errors
///
/// When the text is not TOML.
pub fn without_projects(
    text: &str,
    mut drop: impl FnMut(&str) -> bool,
) -> Result<String, ConfigError> {
    let mut doc: toml_edit::DocumentMut =
        text.parse()
            .map_err(|e: toml_edit::TomlError| ConfigError {
                line: None,
                column: None,
                message: e.message().to_owned(),
            })?;
    let Some(array) = doc
        .get_mut("projects")
        .and_then(toml_edit::Item::as_array_mut)
    else {
        return Ok(doc.to_string());
    };
    // The prefix of the first entry taken in a run: the end of the line
    // before it, which stays, and its spacing on a one-line list.
    let mut kept: Option<String> = None;
    let mut i = 0;
    while i < array.len() {
        if array
            .get(i)
            .and_then(toml_edit::Value::as_str)
            .is_some_and(&mut drop)
        {
            let taken = array.remove(i);
            kept.get_or_insert_with(|| prefix(&taken));
            continue;
        }
        if let Some(before) = kept.take()
            && let Some(value) = array.get_mut(i)
        {
            let own = prefix(value);
            value.decor_mut().set_prefix(rejoin(&before, &own));
        }
        i += 1;
    }
    if let Some(before) = kept {
        let own = array.trailing().as_str().unwrap_or_default().to_owned();
        array.set_trailing(rejoin(&before, &own));
    }
    Ok(doc.to_string())
}

/// The whitespace and comments before a value in an array.
fn prefix(value: &toml_edit::Value) -> String {
    value
        .decor()
        .prefix()
        .and_then(toml_edit::RawString::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `own`, a prefix whose first line belonged to a taken entry, with that
/// line replaced by the end of the line before the taken entry, from
/// `before`, the taken entry's own prefix. On one line, the taken entry's
/// spacing is kept instead.
fn rejoin(before: &str, own: &str) -> String {
    let end = &before[..before.find('\n').unwrap_or(before.len())];
    match own.find('\n') {
        Some(newline) => format!("{end}{}", &own[newline..]),
        None if own.contains('#') => format!("{before}{own}"),
        None => before.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn relative_projects_resolve_against_the_fleet_directory() {
        let config = Config {
            projects: vec!["../code/app".into(), "/c/api".into()],
            ..Config::default()
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
                projects: Vec::new(),
                ..Config::default()
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

    #[test]
    fn taking_a_project_keeps_comments_spelling_and_the_rest() {
        let text = "# mine\nprojects = [\n  \"../api\", # the api\n  \"/c/blog\",\n]\n";
        let edited = without_projects(text, |p| p == "/c/blog");
        assert_eq!(
            edited.as_deref().ok(),
            Some("# mine\nprojects = [\n  \"../api\", # the api\n]\n")
        );
    }

    #[test]
    fn a_taken_entrys_own_comment_goes_with_it() {
        let text =
            "projects = [\n  \"/c/api\", # api\n  \"/c/blog\", # blog\n  \"/c/web\", # web\n]\n";
        let edited = without_projects(text, |p| p == "/c/blog");
        assert_eq!(
            edited.as_deref().ok(),
            Some("projects = [\n  \"/c/api\", # api\n  \"/c/web\", # web\n]\n")
        );
        let first = without_projects(text, |p| p == "/c/api");
        assert_eq!(
            first.as_deref().ok(),
            Some("projects = [\n  \"/c/blog\", # blog\n  \"/c/web\", # web\n]\n")
        );
    }

    #[test]
    fn a_one_line_list_stays_one_line() {
        let edited = without_projects("projects = [\"/c/api\", \"/c/blog\"]\n", |p| p == "/c/api");
        assert_eq!(edited.as_deref().ok(), Some("projects = [\"/c/blog\"]\n"));
    }

    #[test]
    fn taking_every_project_keeps_the_key_and_the_comments() {
        let text = "# mine\nprojects = [\"/c/api\"]\n";
        let edited = without_projects(text, |_| true);
        assert_eq!(edited.as_deref().ok(), Some("# mine\nprojects = []\n"));
    }

    #[test]
    fn a_text_that_is_not_toml_is_an_error() {
        assert!(without_projects("projects = [", |_| true).is_err());
    }
}
