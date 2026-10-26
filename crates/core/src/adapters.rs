//! Adapters named by a version and fetched from fleet-adapters' releases
//! (core.md § Outside tools): what a reference names, what a release's
//! manifest must say, and the protocol rule. Pure: the supervisor moves the
//! bytes and the files.

use std::collections::BTreeMap;
use std::fmt::{self, Write as _};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::status::{self, Mismatch, Status};

/// Where fleet-adapters' release files are downloaded from:
/// `<RELEASES>/<tag>/<file>`.
pub const RELEASES: &str = "https://github.com/tinyish/fleet-adapters/releases/download";

/// fleet-adapters' list of releases, read only to name the latest version
/// of an adapter named without one.
pub const RELEASE_LIST: &str =
    "https://api.github.com/repos/tinyish/fleet-adapters/releases?per_page=100";

/// The variable that puts another base in place of [`RELEASES`], for tests
/// and sandboxes; the latest version is then not looked up.
pub const BASE_VAR: &str = "FLEET_ADAPTERS_URL";

/// The directory, in the state directory, where fetched adapters are kept.
pub const DIR: &str = "adapters";

/// The agent protocols this fleet speaks.
pub const AGENT_PROTOCOLS: &[u32] = &[crate::agent::PROTOCOL];

/// What an adapter reference names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdapterRef {
    /// A program, by its path: run as it is, never fetched.
    Path(PathBuf),
    /// A release of fleet-adapters.
    Named(Named),
}

/// An adapter of fleet-adapters at one version: `claude-code@2.1.285`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named {
    /// The adapter's name: `claude-code`.
    pub name: String,
    /// Its version.
    pub version: Version,
}

/// An adapter's version: its agent's `x.y.z`, and a revision for a fix of
/// the adapter's own (`2.1.285-2`; none is revision 1). Kept as written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    text: String,
    key: ([u64; 3], u32),
}

impl Version {
    /// `text` as a version: `x.y.z`, or `x.y.z-<n>` with n from 2.
    #[must_use]
    pub fn parse(text: &str) -> Option<Version> {
        let (head, revision) = match text.split_once('-') {
            Some((head, rev)) => (head, revision(rev)?),
            None => (text, 1),
        };
        let mut parts = head.split('.').map(number);
        let agent = [parts.next()??, parts.next()??, parts.next()??];
        if parts.next().is_some() {
            return None;
        }
        Some(Version {
            text: text.to_owned(),
            key: (agent, revision),
        })
    }

    /// The version as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.text
    }
}

/// A run of ASCII digits, as a number.
fn number(text: &str) -> Option<u64> {
    (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
        .then(|| text.parse().ok())
        .flatten()
}

/// A revision: digits not starting with 0, from 2.
fn revision(text: &str) -> Option<u32> {
    (!text.starts_with('0'))
        .then(|| number(text))
        .flatten()
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n >= 2)
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Version) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Version) -> std::cmp::Ordering {
        (self.key, &self.text).cmp(&(other.key, &other.text))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// Whether `name` can name an adapter: `[a-z][a-z0-9-]*`.
fn is_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// What `text` names: a path (anything with a `/`), or `<name>@<version>`.
///
/// # Errors
///
/// [`status::Adapter::AdapterUnpinned`] for a bare name (its latest not
/// looked up here), [`status::Adapter::NotAnAdapter`] for anything else.
pub fn parse_ref(text: &str) -> Result<AdapterRef, Status> {
    if text.contains('/') {
        return Ok(AdapterRef::Path(PathBuf::from(text)));
    }
    let not_one = || {
        Status::Adapter(status::Adapter::NotAnAdapter {
            reference: text.to_owned(),
        })
    };
    match text.split_once('@') {
        None if is_name(text) => Err(Status::Adapter(status::Adapter::AdapterUnpinned {
            name: text.to_owned(),
            latest: None,
        })),
        Some((name, version)) if is_name(name) => Version::parse(version)
            .map(|version| {
                AdapterRef::Named(Named {
                    name: name.to_owned(),
                    version,
                })
            })
            .ok_or_else(not_one),
        _ => Err(not_one()),
    }
}

impl Named {
    /// Its release's tag: `claude-code-v2.1.285`.
    #[must_use]
    pub fn tag(&self) -> String {
        format!("{}-v{}", self.name, self.version)
    }

    /// Its program's name: `fleet-claude-code`.
    #[must_use]
    pub fn program(&self) -> String {
        format!("fleet-{}", self.name)
    }

    /// Where it is kept under `adapters`, the kept-adapters directory.
    #[must_use]
    pub fn kept(&self, adapters: &Path) -> PathBuf {
        adapters
            .join(&self.name)
            .join(self.version.as_str())
            .join(self.program())
    }

    /// Its release's manifest, from `base`.
    #[must_use]
    pub fn manifest_url(&self, base: &str) -> String {
        format!("{base}/{}/manifest.json", self.tag())
    }

    /// Its program for `target`, from `base`.
    #[must_use]
    pub fn binary_url(&self, base: &str, target: &str) -> String {
        format!("{base}/{}/{}-{target}", self.tag(), self.program())
    }
}

impl fmt::Display for Named {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}", self.name, self.version)
    }
}

/// The release target of this machine, as fleet-adapters names its builds.
#[must_use]
pub fn target() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Some("aarch64-apple-darwin"),
        ("macos", "x86_64") => Some("x86_64-apple-darwin"),
        ("linux", "aarch64") => Some("aarch64-unknown-linux-musl"),
        ("linux", "x86_64") => Some("x86_64-unknown-linux-musl"),
        _ => None,
    }
}

/// A release's `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Manifest {
    /// The adapter's name.
    pub name: String,
    /// Its version.
    pub version: String,
    /// The protocol it speaks.
    pub protocol: u32,
    /// Each target's SHA-256, in hex.
    pub targets: BTreeMap<String, String>,
}

/// A manifest from its bytes.
///
/// # Errors
///
/// What the parser said, when they are not a manifest.
pub fn read_manifest(bytes: &[u8]) -> Result<Manifest, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

/// The SHA-256 `target`'s build must have, when `manifest`, read from
/// `url`, is `named`'s, speaks a protocol this fleet speaks, and has that
/// build.
///
/// # Errors
///
/// [`status::Adapter::ReleaseMismatch`],
/// [`status::Adapter::ProtocolMismatch`] or [`status::Adapter::NoBuildFor`].
pub fn check_manifest(
    named: &Named,
    manifest: &Manifest,
    url: &str,
    target: &str,
) -> Result<String, Status> {
    if manifest.name != named.name || manifest.version != named.version.as_str() {
        return Err(Status::Adapter(status::Adapter::ReleaseMismatch {
            url: url.to_owned(),
            what: Mismatch::OtherRelease {
                name: manifest.name.clone(),
                version: manifest.version.clone(),
            },
        }));
    }
    check_protocol(&named.to_string(), manifest.protocol, AGENT_PROTOCOLS)?;
    manifest
        .targets
        .get(target)
        .map(|sha| sha.to_ascii_lowercase())
        .ok_or_else(|| {
            Status::Adapter(status::Adapter::NoBuildFor {
                name: named.name.clone(),
                version: named.version.to_string(),
                target: target.to_owned(),
            })
        })
}

/// Whether `adapter`, speaking protocol `speaks`, can be run by a fleet
/// speaking `fleet`.
///
/// # Errors
///
/// [`status::Adapter::ProtocolMismatch`] when `fleet` does not hold
/// `speaks`.
pub fn check_protocol(adapter: &str, speaks: u32, fleet: &[u32]) -> Result<(), Status> {
    if fleet.contains(&speaks) {
        Ok(())
    } else {
        Err(Status::Adapter(status::Adapter::ProtocolMismatch {
            adapter: adapter.to_owned(),
            speaks,
            fleet_speaks: fleet.to_vec(),
        }))
    }
}

/// The SHA-256 of `bytes`, in lowercase hex.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// The latest version of `name` in `releases_json`, GitHub's list of
/// fleet-adapters' releases: by its agent's version, then its revision.
#[must_use]
pub fn latest(name: &str, releases_json: &str) -> Option<String> {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }
    let prefix = format!("{name}-v");
    serde_json::from_str::<Vec<Release>>(releases_json)
        .ok()?
        .iter()
        .filter_map(|r| r.tag_name.strip_prefix(&prefix).and_then(Version::parse))
        .max()
        .map(|v| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::status::Status;
    use std::path::Path;

    const BASE: &str = "https://x/releases/download";

    fn named(text: &str) -> Option<Named> {
        match parse_ref(text) {
            Ok(AdapterRef::Named(n)) => Some(n),
            _ => None,
        }
    }

    #[test]
    fn a_path_is_anything_with_a_slash() {
        for text in ["/a/b", "./x", "bin/fleet-x"] {
            assert_eq!(
                parse_ref(text).ok(),
                Some(AdapterRef::Path(text.into())),
                "{text}"
            );
        }
    }

    #[test]
    fn a_name_and_version_names_a_release() {
        let n = named("claude-code@2.1.285");
        assert!(n.is_some());
        let Some(n) = n else { return };
        assert_eq!(n.to_string(), "claude-code@2.1.285");
        assert_eq!(n.tag(), "claude-code-v2.1.285");
        assert_eq!(n.program(), "fleet-claude-code");
        assert_eq!(
            n.kept(Path::new("/s/adapters")),
            Path::new("/s/adapters/claude-code/2.1.285/fleet-claude-code")
        );
        assert_eq!(
            n.manifest_url(BASE),
            format!("{BASE}/claude-code-v2.1.285/manifest.json")
        );
        assert_eq!(
            n.binary_url(BASE, "aarch64-apple-darwin"),
            format!("{BASE}/claude-code-v2.1.285/fleet-claude-code-aarch64-apple-darwin")
        );
        let Some(fix) = named("claude-code@2.1.285-2") else {
            return assert!(named("claude-code@2.1.285-2").is_some());
        };
        assert_eq!(fix.tag(), "claude-code-v2.1.285-2");
        assert_eq!(
            fix.kept(Path::new("/s/adapters")),
            Path::new("/s/adapters/claude-code/2.1.285-2/fleet-claude-code")
        );
    }

    #[test]
    fn a_bare_name_is_unpinned() {
        assert_eq!(
            parse_ref("claude-code").err(),
            Some(Status::Adapter(status::Adapter::AdapterUnpinned {
                name: "claude-code".into(),
                latest: None
            }))
        );
    }

    #[test]
    fn anything_else_is_not_an_adapter() {
        for text in [
            "",
            "@2.1.285",
            "Claude@2.1.285",
            "claude-code@2.1",
            "claude-code@latest",
            "claude-code@2.1.285-1",
            "claude-code@2.1.285-0",
            "claude-code@2.1.285-02",
            "claude-code@2.1.285-",
            "cc@1.2.3@4",
            "9cc@1.2.3",
        ] {
            assert_eq!(
                parse_ref(text).err(),
                Some(Status::Adapter(status::Adapter::NotAnAdapter {
                    reference: text.into()
                })),
                "{text:?}"
            );
        }
    }

    #[test]
    fn versions_order_by_agent_then_revision() {
        let v = |t: &str| Version::parse(t);
        assert!(v("2.1.285") < v("2.1.285-2"));
        assert!(v("2.1.285-2") < v("2.1.285-10"));
        assert!(v("2.1.285-10") < v("2.1.286"));
        assert!(v("2.1.286") < v("10.0.0"));
        assert_eq!(
            v("2.1.285-2").map(|v| v.to_string()),
            Some("2.1.285-2".into())
        );
    }

    fn manifest(name: &str, version: &str, protocol: u32) -> Manifest {
        Manifest {
            name: name.into(),
            version: version.into(),
            protocol,
            targets: [("aarch64-apple-darwin".to_owned(), "AB12".to_owned())].into(),
        }
    }

    #[test]
    fn a_manifest_for_this_release_gives_this_machines_sha() {
        let n = named("claude-code@2.1.285");
        assert!(n.is_some());
        let Some(n) = n else { return };
        let t = "aarch64-apple-darwin";
        assert_eq!(
            check_manifest(&n, &manifest("claude-code", "2.1.285", 1), "u", t),
            Ok("ab12".into())
        );
        assert_eq!(
            check_manifest(&n, &manifest("codex", "2.1.286", 1), "u", t),
            Err(Status::Adapter(status::Adapter::ReleaseMismatch {
                url: "u".into(),
                what: crate::status::Mismatch::OtherRelease {
                    name: "codex".into(),
                    version: "2.1.286".into()
                }
            }))
        );
        assert_eq!(
            check_manifest(&n, &manifest("claude-code", "2.1.285", 2), "u", t),
            Err(Status::Adapter(status::Adapter::ProtocolMismatch {
                adapter: "claude-code@2.1.285".into(),
                speaks: 2,
                fleet_speaks: vec![1]
            }))
        );
        assert_eq!(
            check_manifest(
                &n,
                &manifest("claude-code", "2.1.285", 1),
                "u",
                "x86_64-apple-darwin"
            ),
            Err(Status::Adapter(status::Adapter::NoBuildFor {
                name: "claude-code".into(),
                version: "2.1.285".into(),
                target: "x86_64-apple-darwin".into()
            }))
        );
    }

    #[test]
    fn a_manifest_reads_from_its_json() {
        let json =
            br#"{"name":"claude-code","version":"2.1.285","protocol":1,"targets":{"t":"ab"}}"#;
        assert_eq!(
            read_manifest(json).map(|m| (m.name, m.protocol, m.targets.len())),
            Ok(("claude-code".into(), 1, 1))
        );
        assert!(read_manifest(b"<html>").is_err());
    }

    #[test]
    fn the_protocol_rule() {
        assert_eq!(check_protocol("a", 1, &[1]), Ok(()));
        for speaks in [0, 2] {
            assert_eq!(
                check_protocol("a", speaks, &[1]),
                Err(Status::Adapter(status::Adapter::ProtocolMismatch {
                    adapter: "a".into(),
                    speaks,
                    fleet_speaks: vec![1]
                }))
            );
        }
    }

    #[test]
    fn sha256_is_lowercase_hex() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn latest_reads_the_highest_release_of_that_adapter() {
        let list = r#"[{"tag_name":"claude-code-v2.1.285","id":1},
            {"tag_name":"claude-code-v2.1.285-2"},{"tag_name":"claude-code-v2.1.290"},
            {"tag_name":"codex-v9.0.0"},{"tag_name":"claude-code-vbroken"}]"#;
        assert_eq!(latest("claude-code", list), Some("2.1.290".into()));
        assert_eq!(latest("pi", list), None);
        assert_eq!(latest("claude-code", "not json"), None);
    }

    #[test]
    fn target_names_this_machine() {
        let triples = [
            "aarch64-apple-darwin",
            "x86_64-apple-darwin",
            "aarch64-unknown-linux-musl",
            "x86_64-unknown-linux-musl",
        ];
        assert!(target().is_some_and(|t| triples.contains(&t)));
    }
}
