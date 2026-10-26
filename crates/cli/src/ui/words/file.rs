//! The words for a path or a fleet's config file that cannot be used.

use std::path::Path;

use fleet_core::status::File;

use crate::ui::output::Ink;

/// Why a path or a config file cannot be used.
pub(super) fn refused(refusal: &File, ink: Ink) -> String {
    match refusal {
        File::Missing { path } => format!("{} does not exist", ink.path(path)),
        File::Unreadable { path, error } => format!("{}: {error}", ink.path(path)),
        File::ConfigDiffers { path, listed } => format!(
            "{} is already here, serving {}",
            ink.path(path),
            ink.paths(listed)
        ),
        File::CannotWrite { path, error } => format!(
            "writing {}: {error}; nothing was registered",
            ink.path(path)
        ),
        File::BadConfig {
            path,
            line,
            column,
            error,
        } => bad_config(path, *line, *column, error, ink),
        File::BadName { path, name } => format!(
            "{} names {}: a name is letters, digits, - and _",
            ink.path(path),
            ink.accent(name)
        ),
        File::UnknownRole { path, seat, role } => format!(
            "{}: the seat {} runs as {}, which is not a role there",
            ink.path(path),
            ink.accent(seat),
            ink.accent(role)
        ),
        File::RoleAdapter { path, role, why } => format!(
            "{}: the role {}: {}",
            ink.path(path),
            ink.accent(role),
            super::refused(why, ink)
        ),
    }
}

/// What to do about it, when there is something to do.
pub(super) fn hint(refusal: &File, ink: Ink) -> Option<String> {
    Some(match refusal {
        File::ConfigDiffers { .. } => format!(
            "run {} without {} to take it up as it is",
            ink.accent("fleet create"),
            ink.accent("--project")
        ),
        File::Missing { .. }
        | File::Unreadable { .. }
        | File::CannotWrite { .. }
        | File::BadConfig { .. }
        | File::BadName { .. }
        | File::UnknownRole { .. }
        | File::RoleAdapter { .. } => return None,
    })
}

/// A fleet's config that does not read, and where when the parser knows.
fn bad_config(
    path: &Path,
    line: Option<usize>,
    column: Option<usize>,
    error: &str,
    ink: Ink,
) -> String {
    match (line, column) {
        (Some(line), Some(column)) => format!(
            "{} does not read: line {line}, column {column}: {error}",
            ink.path(path)
        ),
        _ => format!("{} does not read: {error}", ink.path(path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::ERR;
    use std::path::PathBuf;

    fn plain(s: &str) -> String {
        console::strip_ansi_codes(s).into_owned()
    }

    #[test]
    fn a_differing_file_names_what_it_lists_and_how_to_take_it_up() {
        let differs = File::ConfigDiffers {
            path: PathBuf::from("/code/wing/fleet.toml"),
            listed: vec![PathBuf::from("/code/api"), PathBuf::from("/code/web")],
        };
        assert_eq!(
            plain(&refused(&differs, ERR.piped())),
            "/code/wing/fleet.toml is already here, serving /code/api, /code/web"
        );
        assert_eq!(
            hint(&differs, ERR.piped()).map(|h| plain(&h)).as_deref(),
            Some("run fleet create without --project to take it up as it is")
        );
    }
}
