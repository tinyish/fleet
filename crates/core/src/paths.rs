//! Where tinyfleet keeps what it writes.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// The state file's name, in the state directory.
pub const STATE_FILE: &str = "state.sqlite3";

/// Every file tinyfleet keeps in its state directory, so uninstall deletes
/// those and nothing else.
pub const OWN_FILES: [&str; 6] = [
    STATE_FILE,
    "state.sqlite3-wal",
    "state.sqlite3-shm",
    "supervisor.log",
    "supervisor.sock",
    "supervisor.lock",
];

/// Every directory tinyfleet keeps in its state directory, deleted whole by
/// uninstall: the adapters it fetched.
pub const OWN_DIRS: [&str; 1] = [crate::adapters::DIR];

/// A fleet's config file's name, in the fleet directory.
pub const CONFIG_FILE: &str = "fleet.toml";

/// The operating systems tinyfleet runs on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    /// macOS.
    MacOs,
    /// Linux.
    Linux,
}

impl Os {
    /// The system this binary was built for, if tinyfleet runs on it.
    #[must_use]
    pub fn this() -> Option<Os> {
        if cfg!(target_os = "macos") {
            Some(Os::MacOs)
        } else if cfg!(target_os = "linux") {
            Some(Os::Linux)
        } else {
            None
        }
    }
}

/// Why the state directory could not be worked out.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    feature = "specimen",
    derive(strum::EnumDiscriminants),
    strum_discriminants(name(PathErrorKind), derive(strum::EnumIter))
)]
pub enum PathError {
    /// `HOME` is unset or empty.
    NoHome,
    /// A variable that must hold an absolute path holds a relative one.
    NotAbsolute {
        /// The variable's name.
        var: &'static str,
        /// What it held.
        value: PathBuf,
    },
}

fn non_empty(var: &impl Fn(&str) -> Option<OsString>, name: &str) -> Option<PathBuf> {
    var(name).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// The directory tinyfleet keeps its state in, for one user.
///
/// `FLEET_STATE_DIR` wins when set. Otherwise macOS uses
/// `~/Library/Application Support/fleet` and Linux uses
/// `$XDG_STATE_HOME/fleet` (when absolute) or `~/.local/state/fleet`.
///
/// # Errors
///
/// `NoHome` when the default needs `HOME` and it is unset or empty;
/// `NotAbsolute` when `FLEET_STATE_DIR` is relative.
pub fn state_dir(os: Os, var: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, PathError> {
    if let Some(dir) = non_empty(&var, "FLEET_STATE_DIR") {
        if dir.is_absolute() {
            return Ok(dir);
        }
        return Err(PathError::NotAbsolute {
            var: "FLEET_STATE_DIR",
            value: dir,
        });
    }
    if os == Os::Linux
        && let Some(xdg) = non_empty(&var, "XDG_STATE_HOME").filter(|p| p.is_absolute())
    {
        return Ok(xdg.join("fleet"));
    }
    let home = non_empty(&var, "HOME").ok_or(PathError::NoHome)?;
    Ok(match os {
        Os::MacOs => home.join("Library/Application Support/fleet"),
        Os::Linux => home.join(".local/state/fleet"),
    })
}

/// `path` with `.` and `..` resolved by its text alone, for a path that no
/// longer exists and so cannot be made canonical. `..` at the root stays at
/// the root.
#[must_use]
pub fn lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> + use<> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn override_wins_on_both_systems() {
        let var = env(&[("FLEET_STATE_DIR", "/tmp/f"), ("HOME", "/home/a")]);
        assert_eq!(
            state_dir(Os::MacOs, &var).ok(),
            Some(PathBuf::from("/tmp/f"))
        );
        assert_eq!(
            state_dir(Os::Linux, &var).ok(),
            Some(PathBuf::from("/tmp/f"))
        );
    }

    #[test]
    fn relative_override_is_refused() {
        let var = env(&[("FLEET_STATE_DIR", "rel/dir"), ("HOME", "/home/a")]);
        assert!(matches!(
            state_dir(Os::Linux, &var),
            Err(PathError::NotAbsolute { .. })
        ));
    }

    #[test]
    fn macos_default_is_application_support() {
        let var = env(&[("HOME", "/Users/a")]);
        assert_eq!(
            state_dir(Os::MacOs, &var).ok(),
            Some(PathBuf::from("/Users/a/Library/Application Support/fleet"))
        );
    }

    #[test]
    fn linux_uses_absolute_xdg_state_home() {
        let var = env(&[("XDG_STATE_HOME", "/x/state"), ("HOME", "/home/a")]);
        assert_eq!(
            state_dir(Os::Linux, &var).ok(),
            Some(PathBuf::from("/x/state/fleet"))
        );
    }

    #[test]
    fn linux_ignores_relative_or_empty_xdg_state_home() {
        for xdg in ["", "rel"] {
            let var = env(&[("XDG_STATE_HOME", xdg), ("HOME", "/home/a")]);
            assert_eq!(
                state_dir(Os::Linux, &var).ok(),
                Some(PathBuf::from("/home/a/.local/state/fleet"))
            );
        }
    }

    #[test]
    fn no_home_is_an_error() {
        let var = env(&[]);
        assert!(matches!(state_dir(Os::MacOs, &var), Err(PathError::NoHome)));
        let var = env(&[("HOME", "")]);
        assert!(matches!(state_dir(Os::Linux, &var), Err(PathError::NoHome)));
    }

    #[test]
    fn lexical_resolves_dots_by_text() {
        assert_eq!(
            lexical(Path::new("/c/wing/../blog/./src")),
            PathBuf::from("/c/blog/src")
        );
        assert_eq!(lexical(Path::new("/../c")), PathBuf::from("/c"));
    }

    #[test]
    fn own_files_name_the_state_file_and_the_supervisors_files() {
        for name in [
            STATE_FILE,
            "supervisor.sock",
            "supervisor.lock",
            "supervisor.log",
        ] {
            assert!(OWN_FILES.contains(&name), "{name}");
        }
    }
}
