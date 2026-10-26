//! Adapters, as a request names them: a program by its path, run as it is
//! once it says it speaks this fleet's protocol, or a release of
//! fleet-adapters, fetched the first time and kept under the state
//! directory (core.md § Outside tools). core's `adapters` holds the rules;
//! this moves the bytes and the files.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use fleet_core::adapters::{self, AGENT_PROTOCOLS, AdapterRef, Manifest, Named};
use fleet_core::agent::Verb;
use fleet_core::protocol::Status;
use fleet_core::status::{self, Mismatch};
use fleet_core::tools::{Fetched, ToolError};

use crate::Tools;

/// Where fetched adapters are kept, and where they are fetched from.
pub(crate) struct Store {
    /// The kept-adapters directory, in the state directory.
    dir: PathBuf,
    /// A base of its own in place of fleet-adapters' releases, for tests
    /// and sandboxes (`FLEET_ADAPTERS_URL`).
    base: Option<String>,
}

impl Store {
    /// The store of `state_dir`, fetching from `base` when given.
    pub(crate) fn new(state_dir: &Path, base: Option<String>) -> Store {
        Store {
            dir: state_dir.join(adapters::DIR),
            base,
        }
    }

    fn releases(&self) -> &str {
        self.base.as_deref().unwrap_or(adapters::RELEASES)
    }
}

/// The program `reference` names, ready to run: a path once it says it
/// speaks this fleet's protocol; a release's kept copy, fetched first if
/// it is not kept yet.
pub(crate) fn resolve(store: &Store, tools: &Tools, reference: &str) -> Result<PathBuf, Status> {
    match adapters::parse_ref(reference) {
        Ok(AdapterRef::Path(path)) => speaks(tools, &path, reference).map(|_| path),
        Ok(AdapterRef::Named(named)) => kept(store, tools, &named),
        Err(Status::Adapter(status::Adapter::AdapterUnpinned { name, .. })) => {
            Err(Status::Adapter(status::Adapter::AdapterUnpinned {
                latest: latest(store, tools, &name),
                name,
            }))
        }
        Err(status) => Err(status),
    }
}

/// The protocol `program` speaks, when this fleet speaks it; `adapter` is
/// how it was named.
fn speaks(tools: &Tools, program: &Path, adapter: &str) -> Result<u32, Status> {
    speaks_as(tools, program, program, adapter)
}

/// [`speaks`], a fault naming the program as `shown`: where a build being
/// fetched would be kept, not its temp file.
fn speaks_as(tools: &Tools, program: &Path, shown: &Path, adapter: &str) -> Result<u32, Status> {
    let speaks = tools.agents.protocol(program).map_err(|fault| {
        Status::Session(status::Session::AdapterFailed {
            adapter: shown.to_path_buf(),
            verb: Verb::Protocol,
            fault,
        })
    })?;
    adapters::check_protocol(adapter, speaks.protocol, AGENT_PROTOCOLS)?;
    Ok(speaks.protocol)
}

/// `named`'s kept copy, fetched and checked first if there is none.
fn kept(store: &Store, tools: &Tools, named: &Named) -> Result<PathBuf, Status> {
    let kept = named.kept(&store.dir);
    if kept.is_file() {
        return Ok(kept);
    }
    let dir = kept.parent().unwrap_or(&store.dir);
    fs::create_dir_all(dir).map_err(|e| not_kept(dir, &e))?;
    let target = adapters::target().ok_or_else(|| {
        Status::Adapter(status::Adapter::NoBuildFor {
            name: named.name.clone(),
            version: named.version.to_string(),
            target: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        })
    })?;
    let url = named.manifest_url(store.releases());
    let manifest = Temp::new(dir, "manifest.json");
    if download(tools, &url, &manifest.0)? == Fetched::NotFound {
        return Err(Status::Adapter(status::Adapter::NoSuchRelease {
            name: named.name.clone(),
            version: named.version.to_string(),
        }));
    }
    let read: Manifest = fs::read(&manifest.0)
        .map_err(|e| e.to_string())
        .and_then(|bytes| adapters::read_manifest(&bytes))
        .map_err(|error| {
            Status::Adapter(status::Adapter::BadManifest {
                url: url.clone(),
                error,
            })
        })?;
    let sha = adapters::check_manifest(named, &read, &url, target)?;
    let binary = fetch_binary(store, tools, named, target, dir, &sha)?;
    let build = speaks_as(tools, &binary.0, &kept, &named.to_string())?;
    if build != read.protocol {
        return Err(Status::Adapter(status::Adapter::ReleaseMismatch {
            url,
            what: Mismatch::BuildSpeaks {
                manifest: read.protocol,
                build,
            },
        }));
    }
    fs::rename(&binary.0, &kept).map_err(|e| not_kept(&kept, &e))?;
    Ok(kept)
}

/// `named`'s build for `target`, downloaded beside its place in `dir`,
/// checked against `sha` and made executable.
fn fetch_binary(
    store: &Store,
    tools: &Tools,
    named: &Named,
    target: &str,
    dir: &Path,
    sha: &str,
) -> Result<Temp, Status> {
    let url = named.binary_url(store.releases(), target);
    let binary = Temp::new(dir, &named.program());
    if download(tools, &url, &binary.0)? == Fetched::NotFound {
        return Err(Status::Adapter(status::Adapter::ReleaseMismatch {
            url,
            what: Mismatch::BuildMissing,
        }));
    }
    let bytes = fs::read(&binary.0).map_err(|e| not_kept(&binary.0, &e))?;
    let got = adapters::sha256_hex(&bytes);
    if got != sha {
        return Err(Status::Adapter(status::Adapter::ChecksumMismatch {
            name: named.name.clone(),
            version: named.version.to_string(),
            expected: sha.to_owned(),
            got,
        }));
    }
    fs::set_permissions(&binary.0, fs::Permissions::from_mode(0o755))
        .map_err(|e| not_kept(&binary.0, &e))?;
    Ok(binary)
}

/// Downloads `url` to `to`, a failure as the refusal it is.
fn download(tools: &Tools, url: &str, to: &Path) -> Result<Fetched, Status> {
    tools.net.download(url, to).map_err(|e| match e {
        ToolError::CouldNotRun { error, .. } => Status::Adapter(status::Adapter::NoDownloader {
            error: error.to_string(),
        }),
        ToolError::Failed { said, .. } => Status::Adapter(status::Adapter::DownloadFailed {
            url: url.to_owned(),
            said,
        }),
        ToolError::File { path, error, .. } => not_kept(&path, &error),
        // The network's driver answers no other kind of failure.
        _ => Status::Adapter(status::Adapter::DownloadFailed {
            url: url.to_owned(),
            said: String::new(),
        }),
    })
}

/// The latest release of `name`, from fleet-adapters' list of releases;
/// none with a base of the store's own, or when the list cannot be read.
fn latest(store: &Store, tools: &Tools, name: &str) -> Option<String> {
    if store.base.is_some() {
        return None;
    }
    fs::create_dir_all(&store.dir).ok()?;
    let list = Temp::new(&store.dir, "releases.json");
    match tools.net.download(adapters::RELEASE_LIST, &list.0).ok()? {
        Fetched::Saved => adapters::latest(name, &fs::read_to_string(&list.0).ok()?),
        Fetched::NotFound => None,
    }
}

fn not_kept(path: &Path, error: &io::Error) -> Status {
    Status::Adapter(status::Adapter::AdapterNotKept {
        path: path.to_path_buf(),
        error: error.to_string(),
    })
}

/// A file being fetched, `.<file>-<pid>-<n>` beside where it will be kept,
/// removed when dropped: once renamed into place, there is nothing left to
/// remove.
struct Temp(PathBuf);

static NEXT: AtomicU32 = AtomicU32::new(0);

impl Temp {
    fn new(dir: &Path, file: &str) -> Temp {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        Temp(dir.join(format!(".{file}-{}-{n}", std::process::id())))
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    use fleet_core::adapters::{self, RELEASE_LIST};
    use fleet_core::agent::{self, AdapterFault, Launched, Readings, Speaks, Verb};
    use fleet_core::protocol::Status;
    use fleet_core::tools::{AgentAdapters, Multiplexer, Pane, ToolError};

    use crate::Tools;
    use crate::testing::{BINARY, FakeNet};

    const BASE: &str = "https://r";

    /// Agent adapters that all speak `protocol`, or fail to say.
    struct Speaking(Result<u32, AdapterFault>);

    impl AgentAdapters for Speaking {
        fn launch(&self, _: &Path, _: &agent::Launch) -> Result<Launched, AdapterFault> {
            Err(AdapterFault::BadAnswer {
                error: "not here".into(),
            })
        }
        fn read(&self, _: &Path, _: &agent::Read) -> Result<Readings, AdapterFault> {
            Err(AdapterFault::BadAnswer {
                error: "not here".into(),
            })
        }
        fn protocol(&self, _: &Path) -> Result<Speaks, AdapterFault> {
            self.0.clone().map(|protocol| Speaks { protocol })
        }
    }

    struct NoMux;

    impl Multiplexer for NoMux {
        fn launch(&self, _: &str, _: &Path, _: &Launched) -> Result<String, ToolError> {
            Ok("%0".into())
        }
        fn panes(&self) -> Result<Vec<Pane>, ToolError> {
            Ok(vec![])
        }
        fn stop(&self, _: &str) -> Result<(), ToolError> {
            Ok(())
        }
    }

    fn tools(net: FakeNet, speaks: Result<u32, AdapterFault>) -> Tools {
        Tools {
            mux: Box::new(NoMux),
            agents: Box::new(Speaking(speaks)),
            net: Box::new(net),
        }
    }

    fn release(protocol: u32, hash: &str) -> BTreeMap<String, Vec<u8>> {
        crate::testing::release(BASE, protocol, hash)
    }

    fn good() -> BTreeMap<String, Vec<u8>> {
        crate::testing::good_release(BASE)
    }

    /// A state directory of the test's own, deleted at the end.
    struct Dir(std::path::PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o700));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn state_dir(name: &str) -> Dir {
        let d =
            std::env::temp_dir().join(format!("fleet-sup-adapters-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let _ = fs::create_dir_all(&d);
        Dir(d)
    }

    fn store(dir: &Dir) -> Store {
        Store::new(&dir.0, Some(BASE.into()))
    }

    fn kept(dir: &Dir) -> std::path::PathBuf {
        dir.0.join("adapters/claude-code/2.1.285/fleet-claude-code")
    }

    /// Files whose names start with `.`, left anywhere under the adapters.
    fn leftovers(dir: &Dir) -> Vec<String> {
        fn walk(p: &Path, out: &mut Vec<String>) {
            for e in fs::read_dir(p).into_iter().flatten().flatten() {
                let path = e.path();
                if path.is_dir() {
                    walk(&path, out);
                } else if e.file_name().to_string_lossy().starts_with('.') {
                    out.push(path.display().to_string());
                }
            }
        }
        let mut out = vec![];
        walk(&dir.0.join("adapters"), &mut out);
        out
    }

    #[test]
    fn a_named_adapter_is_fetched_once_then_kept() {
        let d = state_dir("once");
        let net = Arc::new(FakeNet {
            files: good(),
            ..FakeNet::default()
        });
        let t = Tools {
            mux: Box::new(NoMux),
            agents: Box::new(Speaking(Ok(1))),
            net: Box::new(SharedNet(Arc::clone(&net))),
        };
        assert_eq!(resolve(&store(&d), &t, "claude-code@2.1.285"), Ok(kept(&d)));
        assert_eq!(fs::read(kept(&d)).ok().as_deref(), Some(BINARY));
        let mode = fs::metadata(kept(&d))
            .map(|m| m.permissions().mode() & 0o777)
            .ok();
        assert_eq!(mode, Some(0o755));
        assert_eq!(net.count(), 2);
        assert_eq!(resolve(&store(&d), &t, "claude-code@2.1.285"), Ok(kept(&d)));
        assert_eq!(net.count(), 2, "a kept adapter is not fetched again");
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    /// A network a test keeps a handle on.
    struct SharedNet(Arc<FakeNet>);

    impl fleet_core::tools::Network for SharedNet {
        fn download(&self, url: &str, to: &Path) -> Result<fleet_core::tools::Fetched, ToolError> {
            self.0.download(url, to)
        }
        fn version(&self) -> Result<String, ToolError> {
            self.0.version()
        }
    }

    #[test]
    fn a_missing_release_is_no_such_release() {
        let d = state_dir("missing");
        assert_eq!(
            resolve(
                &store(&d),
                &tools(FakeNet::default(), Ok(1)),
                "claude-code@2.1.285"
            ),
            Err(Status::Adapter(status::Adapter::NoSuchRelease {
                name: "claude-code".into(),
                version: "2.1.285".into()
            }))
        );
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn a_manifest_for_another_release_is_bad_and_nothing_else_downloads() {
        let d = state_dir("other");
        let mut files = good();
        let url = format!("{BASE}/claude-code-v2.1.285/manifest.json");
        files.insert(
            url,
            br#"{"name":"codex","version":"2.1.285","protocol":1,"targets":{}}"#.to_vec(),
        );
        let net = Arc::new(FakeNet {
            files,
            ..FakeNet::default()
        });
        let t = Tools {
            mux: Box::new(NoMux),
            agents: Box::new(Speaking(Ok(1))),
            net: Box::new(SharedNet(Arc::clone(&net))),
        };
        assert!(matches!(
            resolve(&store(&d), &t, "claude-code@2.1.285"),
            Err(Status::Adapter(status::Adapter::ReleaseMismatch {
                what: fleet_core::status::Mismatch::OtherRelease { .. },
                ..
            }))
        ));
        assert_eq!(net.count(), 1);
        assert!(!kept(&d).exists());
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn a_manifest_that_does_not_read_is_bad() {
        let d = state_dir("garbage");
        let mut files = good();
        files.insert(
            format!("{BASE}/claude-code-v2.1.285/manifest.json"),
            b"<html>".to_vec(),
        );
        assert!(matches!(
            resolve(
                &store(&d),
                &tools(
                    FakeNet {
                        files,
                        ..FakeNet::default()
                    },
                    Ok(1)
                ),
                "claude-code@2.1.285"
            ),
            Err(Status::Adapter(status::Adapter::BadManifest { .. }))
        ));
    }

    #[test]
    fn a_newer_protocol_is_refused_before_the_binary_downloads() {
        let d = state_dir("newer");
        let net = Arc::new(FakeNet {
            files: release(2, &adapters::sha256_hex(BINARY)),
            ..FakeNet::default()
        });
        let t = Tools {
            mux: Box::new(NoMux),
            agents: Box::new(Speaking(Ok(2))),
            net: Box::new(SharedNet(Arc::clone(&net))),
        };
        assert_eq!(
            resolve(&store(&d), &t, "claude-code@2.1.285"),
            Err(Status::Adapter(status::Adapter::ProtocolMismatch {
                adapter: "claude-code@2.1.285".into(),
                speaks: 2,
                fleet_speaks: vec![1]
            }))
        );
        assert_eq!(net.count(), 1);
    }

    #[test]
    fn a_checksum_mismatch_keeps_nothing() {
        let d = state_dir("sha");
        let files = release(1, &"0".repeat(64));
        let got = resolve(
            &store(&d),
            &tools(
                FakeNet {
                    files,
                    ..FakeNet::default()
                },
                Ok(1),
            ),
            "claude-code@2.1.285",
        );
        assert_eq!(
            got,
            Err(Status::Adapter(status::Adapter::ChecksumMismatch {
                name: "claude-code".into(),
                version: "2.1.285".into(),
                expected: "0".repeat(64),
                got: adapters::sha256_hex(BINARY),
            }))
        );
        assert!(!kept(&d).exists());
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn a_binary_speaking_another_protocol_keeps_nothing() {
        let d = state_dir("binary");
        let got = resolve(
            &store(&d),
            &tools(
                FakeNet {
                    files: good(),
                    ..FakeNet::default()
                },
                Ok(2),
            ),
            "claude-code@2.1.285",
        );
        assert!(
            matches!(
                got,
                Err(Status::Adapter(status::Adapter::ProtocolMismatch {
                    speaks: 2,
                    ..
                }))
            ),
            "{got:?}"
        );
        assert!(!kept(&d).exists());
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn a_binary_the_release_lacks_is_a_failed_download() {
        let d = state_dir("lacks");
        let mut files = good();
        files.retain(|url, _| url.ends_with("manifest.json"));
        assert!(matches!(
            resolve(
                &store(&d),
                &tools(
                    FakeNet {
                        files,
                        ..FakeNet::default()
                    },
                    Ok(1)
                ),
                "claude-code@2.1.285"
            ),
            Err(Status::Adapter(status::Adapter::ReleaseMismatch {
                what: fleet_core::status::Mismatch::BuildMissing,
                ..
            }))
        ));
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn a_bare_name_is_refused_with_the_latest() {
        let d = state_dir("bare");
        let list = br#"[{"tag_name":"claude-code-v2.1.285"},{"tag_name":"claude-code-v2.1.290"}]"#;
        let net = FakeNet {
            files: BTreeMap::from([(RELEASE_LIST.to_owned(), list.to_vec())]),
            ..FakeNet::default()
        };
        assert_eq!(
            resolve(&Store::new(&d.0, None), &tools(net, Ok(1)), "claude-code"),
            Err(Status::Adapter(status::Adapter::AdapterUnpinned {
                name: "claude-code".into(),
                latest: Some("2.1.290".into())
            }))
        );
        let net = Arc::new(FakeNet::default());
        let t = Tools {
            mux: Box::new(NoMux),
            agents: Box::new(Speaking(Ok(1))),
            net: Box::new(SharedNet(Arc::clone(&net))),
        };
        assert_eq!(
            resolve(&store(&d), &t, "claude-code"),
            Err(Status::Adapter(status::Adapter::AdapterUnpinned {
                name: "claude-code".into(),
                latest: None
            }))
        );
        assert_eq!(
            net.count(),
            0,
            "with a base of its own, the latest is not looked up"
        );
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn a_path_adapter_is_asked_its_protocol() {
        let d = state_dir("path");
        let s = store(&d);
        assert_eq!(
            resolve(&s, &tools(FakeNet::default(), Ok(1)), "/a/mine"),
            Ok("/a/mine".into())
        );
        assert_eq!(
            resolve(&s, &tools(FakeNet::default(), Ok(2)), "/a/mine"),
            Err(Status::Adapter(status::Adapter::ProtocolMismatch {
                adapter: "/a/mine".into(),
                speaks: 2,
                fleet_speaks: vec![1]
            }))
        );
        let fault = AdapterFault::Exited {
            code: Some(2),
            said: "unknown verb".into(),
        };
        assert_eq!(
            resolve(
                &s,
                &tools(FakeNet::default(), Err(fault.clone())),
                "/a/mine"
            ),
            Err(Status::Session(status::Session::AdapterFailed {
                adapter: "/a/mine".into(),
                verb: Verb::Protocol,
                fault
            }))
        );
    }

    #[test]
    fn a_fetched_build_that_cannot_say_its_protocol_is_named_by_where_it_would_be_kept() {
        let d = state_dir("killed");
        let fault = AdapterFault::Exited {
            code: None,
            said: String::new(),
        };
        let net = FakeNet {
            files: good(),
            ..FakeNet::default()
        };
        assert_eq!(
            resolve(
                &store(&d),
                &tools(net, Err(fault.clone())),
                "claude-code@2.1.285"
            ),
            Err(Status::Session(status::Session::AdapterFailed {
                adapter: kept(&d),
                verb: Verb::Protocol,
                fault
            }))
        );
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn a_state_directory_that_cannot_be_written_keeps_nothing() {
        let d = state_dir("readonly");
        let _ = fs::set_permissions(&d.0, fs::Permissions::from_mode(0o500));
        let got = resolve(
            &store(&d),
            &tools(
                FakeNet {
                    files: good(),
                    ..FakeNet::default()
                },
                Ok(1),
            ),
            "claude-code@2.1.285",
        );
        assert!(
            matches!(
                got,
                Err(Status::Adapter(status::Adapter::AdapterNotKept { .. }))
            ),
            "{got:?}"
        );
    }

    #[test]
    fn two_fetches_of_one_adapter_both_keep_it() {
        let d = state_dir("race");
        let t = Arc::new(tools(
            FakeNet {
                files: good(),
                delay: Duration::from_millis(50),
                ..FakeNet::default()
            },
            Ok(1),
        ));
        let s = Arc::new(store(&d));
        let racers: Vec<_> = (0..2)
            .map(|_| {
                let (t, s) = (Arc::clone(&t), Arc::clone(&s));
                std::thread::spawn(move || resolve(&s, &t, "claude-code@2.1.285"))
            })
            .collect();
        for racer in racers {
            assert_eq!(racer.join().ok(), Some(Ok(kept(&d))));
        }
        assert_eq!(fs::read(kept(&d)).ok().as_deref(), Some(BINARY));
        assert_eq!(leftovers(&d), Vec::<String>::new());
    }

    #[test]
    fn no_curl_is_no_downloader() {
        let d = state_dir("nocurl");
        assert!(matches!(
            resolve(
                &store(&d),
                &tools(
                    FakeNet {
                        no_curl: true,
                        ..FakeNet::default()
                    },
                    Ok(1)
                ),
                "claude-code@2.1.285"
            ),
            Err(Status::Adapter(status::Adapter::NoDownloader { .. }))
        ));
    }
}
