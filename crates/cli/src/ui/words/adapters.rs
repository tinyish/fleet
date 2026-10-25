//! The words for what goes wrong naming and fetching adapters.

use fleet_core::protocol;
use fleet_core::status::Mismatch;

use crate::ui::output::Ink;

/// Why an adapter could not be named or fetched; none for a refusal that
/// is not about that.
pub(super) fn refused(status: &protocol::Status, ink: Ink) -> Option<String> {
    use protocol::Status as S;
    Some(match status {
        S::AdapterUnpinned {
            name,
            latest: Some(latest),
        } => format!(
            "the adapter {} needs a version: the latest is {latest}, so name it {}",
            ink.accent(name),
            ink.accent(format!("{name}@{latest}"))
        ),
        S::AdapterUnpinned { name, latest: None } => format!(
            "the adapter {} needs a version, as {}",
            ink.accent(name),
            ink.accent(format!("{name}@<version>"))
        ),
        S::NotAnAdapter { reference } => format!(
            "{} is not an adapter: name one by its path, or as <name>@<version>",
            ink.accent(reference)
        ),
        S::NoSuchRelease { name, version } => {
            format!(
                "fleet-adapters has no release {} {version}",
                ink.accent(name)
            )
        }
        S::BadManifest { url, error } => {
            format!("the manifest at {} does not read: {error}", ink.accent(url))
        }
        S::NoBuildFor {
            name,
            version,
            target,
        } => format!(
            "{} {version} has no build for this machine ({target})",
            ink.accent(name)
        ),
        S::ChecksumMismatch {
            name,
            version,
            expected,
            got,
        } => format!(
            "the download of {} {version} is not the one released: its SHA-256 is {got}, not {expected}",
            ink.accent(name)
        ),
        S::ProtocolMismatch {
            adapter,
            speaks,
            fleet_speaks,
        } => format!(
            "the adapter {} speaks agent protocol {speaks}, and this fleet speaks {}: {}",
            ink.accent(adapter),
            either(fleet_speaks),
            if fleet_speaks.iter().all(|n| speaks > n) {
                "update fleet"
            } else {
                "update the adapter"
            }
        ),
        S::DownloadFailed { url, said } if said.is_empty() => {
            format!("could not download {}", ink.accent(url))
        }
        S::DownloadFailed { url, said } => {
            format!("could not download {}: {said}", ink.accent(url))
        }
        S::ReleaseMismatch { url, what } => mismatch(url, what, ink),
        S::NoDownloader { error } => {
            format!("{} could not be run: {error}", ink.accent("curl"))
        }
        S::AdapterNotKept { path, error } => {
            format!("could not keep the adapter in {}: {error}", ink.path(path))
        }
        _ => return None,
    })
}

/// How the release at `url` does not hold together.
fn mismatch(url: &str, what: &Mismatch, ink: Ink) -> String {
    match what {
        Mismatch::OtherRelease { name, version } => format!(
            "the release at {} is not the one asked for: its manifest is {}",
            ink.accent(url),
            ink.accent(format!("{name}@{version}"))
        ),
        Mismatch::BuildSpeaks { manifest, build } => format!(
            "the release at {} does not hold together: its manifest says protocol {manifest}, its build speaks {build}",
            ink.accent(url)
        ),
        Mismatch::BuildMissing => format!(
            "the release's manifest lists {}, but it is not there",
            ink.accent(url)
        ),
    }
}

/// `1`, `1 or 2`, `1, 2 or 3`.
fn either(numbers: &[u32]) -> String {
    let words: Vec<String> = numbers.iter().map(u32::to_string).collect();
    match words.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => "none".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::output::OUT;
    use fleet_core::protocol::Status as S;

    fn says(status: &S) -> String {
        refused(status, OUT.piped())
            .map(|w| console::strip_ansi_codes(&w).into_owned())
            .unwrap_or_default()
    }

    fn s(text: &str) -> String {
        text.to_owned()
    }

    #[test]
    fn an_unpinned_adapter_names_its_latest_when_known() {
        assert_eq!(
            says(&S::AdapterUnpinned {
                name: s("claude-code"),
                latest: Some(s("2.1.285"))
            }),
            "the adapter claude-code needs a version: the latest is 2.1.285, so name it claude-code@2.1.285"
        );
        assert_eq!(
            says(&S::AdapterUnpinned {
                name: s("claude-code"),
                latest: None
            }),
            "the adapter claude-code needs a version, as claude-code@<version>"
        );
    }

    #[test]
    fn each_fetching_refusal_says_what_and_why() {
        assert_eq!(
            says(&S::NotAnAdapter {
                reference: s("cc@x")
            }),
            "cc@x is not an adapter: name one by its path, or as <name>@<version>"
        );
        assert_eq!(
            says(&S::NoSuchRelease {
                name: s("claude-code"),
                version: s("9.9.9")
            }),
            "fleet-adapters has no release claude-code 9.9.9"
        );
        assert_eq!(
            says(&S::BadManifest {
                url: s("https://u/m.json"),
                error: s("EOF")
            }),
            "the manifest at https://u/m.json does not read: EOF"
        );
        assert_eq!(
            says(&S::NoBuildFor {
                name: s("claude-code"),
                version: s("2.1.285"),
                target: s("riscv")
            }),
            "claude-code 2.1.285 has no build for this machine (riscv)"
        );
        assert_eq!(
            says(&S::ChecksumMismatch {
                name: s("claude-code"),
                version: s("2.1.285"),
                expected: s("aa"),
                got: s("bb")
            }),
            "the download of claude-code 2.1.285 is not the one released: its SHA-256 is bb, not aa"
        );
        assert_eq!(
            says(&S::DownloadFailed {
                url: s("https://u/x"),
                said: s("curl: (6) Could not resolve host")
            }),
            "could not download https://u/x: curl: (6) Could not resolve host"
        );
        assert_eq!(
            says(&S::AdapterNotKept {
                path: "/st/adapters".into(),
                error: s("Permission denied")
            }),
            "could not keep the adapter in /st/adapters: Permission denied"
        );
        assert_eq!(
            says(&S::NoDownloader {
                error: s("No such file or directory")
            }),
            "curl could not be run: No such file or directory"
        );
    }

    #[test]
    fn a_release_that_does_not_hold_together_says_how() {
        use fleet_core::status::Mismatch;
        let at = |what| {
            says(&S::ReleaseMismatch {
                url: s("https://u/x"),
                what,
            })
        };
        assert_eq!(
            at(Mismatch::OtherRelease {
                name: s("codex"),
                version: s("1.0.0")
            }),
            "the release at https://u/x is not the one asked for: its manifest is codex@1.0.0"
        );
        assert_eq!(
            at(Mismatch::BuildSpeaks {
                manifest: 1,
                build: 2
            }),
            "the release at https://u/x does not hold together: its manifest says protocol 1, its build speaks 2"
        );
        assert_eq!(
            at(Mismatch::BuildMissing),
            "the release's manifest lists https://u/x, but it is not there"
        );
        assert_eq!(
            says(&S::DownloadFailed {
                url: s("https://u/x"),
                said: String::new()
            }),
            "could not download https://u/x"
        );
    }

    #[test]
    fn a_protocol_mismatch_says_which_side_to_update() {
        assert_eq!(
            says(&S::ProtocolMismatch {
                adapter: s("claude-code@3.0.0"),
                speaks: 2,
                fleet_speaks: vec![1]
            }),
            "the adapter claude-code@3.0.0 speaks agent protocol 2, and this fleet speaks 1: update fleet"
        );
        assert_eq!(
            says(&S::ProtocolMismatch {
                adapter: s("/a/mine"),
                speaks: 1,
                fleet_speaks: vec![2, 3]
            }),
            "the adapter /a/mine speaks agent protocol 1, and this fleet speaks 2 or 3: update the adapter"
        );
    }

    #[test]
    fn other_refusals_are_not_this_modules() {
        assert_eq!(
            refused(&S::NotServed { path: "/x".into() }, OUT.piped()),
            None
        );
    }
}
