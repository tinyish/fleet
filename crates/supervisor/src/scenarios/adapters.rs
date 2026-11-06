//! How a launch's adapter is found: a path, asked which protocol it
//! speaks, or a release of fleet-adapters, fetched the first time, checked,
//! and kept under the state directory.

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::PoisonError;
use std::thread;
use std::time::Duration;

use fleet_core::adapters::sha256_hex;
use fleet_core::agent::{AdapterFault, Verb};
use fleet_core::protocol::{Reply, Response, Status, response};
use fleet_core::status::{self, Mismatch};

use super::{Fakes, Rig, bare, rig_with, spawned};
use crate::testing::{BINARY, FakeAgents, FakeNet, good_release, release};

/// Where the fakes serve releases from: the rig's store fetches from here.
const BASE: &str = "https://r";

/// A rig whose `code/app` was made a default fleet with one role, `w`, of
/// release claude-code 2.1.285, fetched through `net`, by adapters that
/// all speak `speaks`.
fn named(name: &str, net: FakeNet, speaks: Result<u32, AdapterFault>) -> Rig {
    let r = bare(
        name,
        Fakes {
            agents: FakeAgents {
                speaks: Some(speaks),
                ..FakeAgents::default()
            },
            net,
            ..Fakes::default()
        },
    );
    let config = "[roles.w]\nadapter = \"claude-code@2.1.285\"\n";
    assert!(fs::write(r.app.join("fleet.toml"), config).is_ok());
    let created = r.create(&r.app, &[]);
    assert!(matches!(created, Reply::Response(_)), "{created:?}");
    r
}

fn serving(files: BTreeMap<String, Vec<u8>>) -> FakeNet {
    FakeNet {
        files,
        ..FakeNet::default()
    }
}

fn spawn(r: &Rig) -> Reply {
    r.spawn_from(&r.app, "w", None)
}

/// Where the release's build is kept.
fn kept(r: &Rig) -> PathBuf {
    r.root
        .join("st/adapters/claude-code/2.1.285/fleet-claude-code")
}

/// Files whose names start with `.`, left anywhere under the kept
/// adapters.
fn leftovers(r: &Rig) -> Vec<String> {
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
    walk(&r.root.join("st/adapters"), &mut out);
    out
}

fn refusal(reply: Reply) -> Option<Status> {
    match reply {
        Reply::Status(status) => Some(status),
        Reply::Response(_) => None,
    }
}

#[test]
fn a_named_adapter_is_fetched_once_then_kept_and_launched() {
    let r = named("adapter-once", serving(good_release(BASE)), Ok(1));
    assert_ne!(spawned(&spawn(&r)), "");
    assert_eq!(fs::read(kept(&r)).ok().as_deref(), Some(BINARY));
    let mode = fs::metadata(kept(&r))
        .map(|m| m.permissions().mode() & 0o777)
        .ok();
    assert_eq!(mode, Some(0o755));
    assert_eq!(r.net.count(), 2);
    assert_ne!(spawned(&spawn(&r)), "");
    assert_eq!(r.net.count(), 2, "a kept adapter is not fetched again");
    let launches = r
        .agents
        .launches
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    assert_eq!(launches, [kept(&r), kept(&r)]);
    let adapters: Vec<PathBuf> = r
        .status(&r.app)
        .fleets
        .into_iter()
        .flat_map(|f| f.spawned)
        .map(|s| s.session.adapter)
        .collect();
    assert_eq!(adapters, [kept(&r), kept(&r)]);
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn a_missing_release_is_no_such_release() {
    let r = named("adapter-missing", FakeNet::default(), Ok(1));
    assert_eq!(
        refusal(spawn(&r)),
        Some(Status::Adapter(status::Adapter::NoSuchRelease {
            name: "claude-code".into(),
            version: "2.1.285".into()
        }))
    );
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn a_manifest_for_another_release_is_bad_and_nothing_else_downloads() {
    let mut files = good_release(BASE);
    files.insert(
        format!("{BASE}/claude-code-v2.1.285/manifest.json"),
        br#"{"name":"codex","version":"2.1.285","protocol":1,"targets":{}}"#.to_vec(),
    );
    let r = named("adapter-other", serving(files), Ok(1));
    let got = refusal(spawn(&r));
    assert!(
        matches!(
            got,
            Some(Status::Adapter(status::Adapter::ReleaseMismatch {
                what: Mismatch::OtherRelease { .. },
                ..
            }))
        ),
        "{got:?}"
    );
    assert_eq!(r.net.count(), 1);
    assert!(!kept(&r).exists());
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn a_manifest_that_does_not_read_is_bad() {
    let mut files = good_release(BASE);
    files.insert(
        format!("{BASE}/claude-code-v2.1.285/manifest.json"),
        b"<html>".to_vec(),
    );
    let r = named("adapter-garbage", serving(files), Ok(1));
    let got = refusal(spawn(&r));
    assert!(
        matches!(
            got,
            Some(Status::Adapter(status::Adapter::BadManifest { .. }))
        ),
        "{got:?}"
    );
}

#[test]
fn a_newer_protocol_is_refused_before_the_binary_downloads() {
    let files = release(BASE, 2, &sha256_hex(BINARY));
    let r = named("adapter-newer", serving(files), Ok(2));
    assert_eq!(
        refusal(spawn(&r)),
        Some(Status::Adapter(status::Adapter::ProtocolMismatch {
            adapter: "claude-code@2.1.285".into(),
            speaks: 2,
            fleet_speaks: vec![1]
        }))
    );
    assert_eq!(r.net.count(), 1);
}

#[test]
fn a_checksum_mismatch_keeps_nothing() {
    let r = named(
        "adapter-sha",
        serving(release(BASE, 1, &"0".repeat(64))),
        Ok(1),
    );
    assert_eq!(
        refusal(spawn(&r)),
        Some(Status::Adapter(status::Adapter::ChecksumMismatch {
            name: "claude-code".into(),
            version: "2.1.285".into(),
            expected: "0".repeat(64),
            got: sha256_hex(BINARY),
        }))
    );
    assert!(!kept(&r).exists());
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn a_binary_speaking_another_protocol_keeps_nothing() {
    let r = named("adapter-binary", serving(good_release(BASE)), Ok(2));
    let got = refusal(spawn(&r));
    assert!(
        matches!(
            got,
            Some(Status::Adapter(status::Adapter::ProtocolMismatch {
                speaks: 2,
                ..
            }))
        ),
        "{got:?}"
    );
    assert!(!kept(&r).exists());
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn a_binary_the_release_lacks_is_a_failed_download() {
    let mut files = good_release(BASE);
    files.retain(|url, _| url.ends_with("manifest.json"));
    let r = named("adapter-lacks", serving(files), Ok(1));
    let got = refusal(spawn(&r));
    assert!(
        matches!(
            got,
            Some(Status::Adapter(status::Adapter::ReleaseMismatch {
                what: Mismatch::BuildMissing,
                ..
            }))
        ),
        "{got:?}"
    );
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn a_path_adapter_is_asked_its_protocol() {
    let fault = AdapterFault::Exited {
        code: Some(2),
        said: "unknown verb".into(),
    };
    let cases = [
        ("1", Ok(1), None),
        (
            "2",
            Ok(2),
            Some(Status::Adapter(status::Adapter::ProtocolMismatch {
                adapter: "/a/claude".into(),
                speaks: 2,
                fleet_speaks: vec![1],
            })),
        ),
        (
            "fault",
            Err(fault.clone()),
            Some(Status::Session(status::Session::AdapterFailed {
                adapter: "/a/claude".into(),
                verb: Verb::Protocol,
                fault,
            })),
        ),
    ];
    for (what, speaks, refused) in cases {
        let r = rig_with(
            &format!("adapter-path-{what}"),
            Fakes {
                agents: FakeAgents {
                    speaks: Some(speaks),
                    ..FakeAgents::default()
                },
                ..Fakes::default()
            },
        );
        let reply = spawn(&r);
        match refused {
            None => assert!(
                matches!(
                    reply,
                    Reply::Response(Response::Spawn(response::Spawn::Spawned { .. }))
                ),
                "{what}: {reply:?}"
            ),
            Some(why) => assert_eq!(reply, Reply::Status(why), "{what}"),
        }
    }
}

#[test]
fn a_fetched_build_that_cannot_say_its_protocol_is_named_by_where_it_would_be_kept() {
    let fault = AdapterFault::Exited {
        code: None,
        said: String::new(),
    };
    let r = named(
        "adapter-killed",
        serving(good_release(BASE)),
        Err(fault.clone()),
    );
    assert_eq!(
        refusal(spawn(&r)),
        Some(Status::Session(status::Session::AdapterFailed {
            adapter: kept(&r),
            verb: Verb::Protocol,
            fault
        }))
    );
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn a_state_directory_that_cannot_be_written_keeps_nothing() {
    let r = named("adapter-readonly", serving(good_release(BASE)), Ok(1));
    let adapters = r.root.join("st/adapters");
    assert!(fs::create_dir_all(&adapters).is_ok());
    let _ = fs::set_permissions(&adapters, fs::Permissions::from_mode(0o500));
    let got = refusal(spawn(&r));
    let _ = fs::set_permissions(&adapters, fs::Permissions::from_mode(0o700));
    assert!(
        matches!(
            got,
            Some(Status::Adapter(status::Adapter::AdapterNotKept { .. }))
        ),
        "{got:?}"
    );
}

#[test]
fn two_fetches_of_one_adapter_both_keep_it() {
    let net = FakeNet {
        files: good_release(BASE),
        delay: Duration::from_millis(50),
        ..FakeNet::default()
    };
    let r = named("adapter-race", net, Ok(1));
    let replies: Vec<Option<Reply>> = thread::scope(|s| {
        let racers: Vec<_> = (0..2).map(|_| s.spawn(|| spawn(&r))).collect();
        racers.into_iter().map(|t| t.join().ok()).collect()
    });
    for reply in replies {
        assert!(
            matches!(reply, Some(Reply::Response(Response::Spawn(_)))),
            "{reply:?}"
        );
    }
    assert_eq!(fs::read(kept(&r)).ok().as_deref(), Some(BINARY));
    assert_eq!(leftovers(&r), Vec::<String>::new());
}

#[test]
fn no_curl_is_no_downloader() {
    let net = FakeNet {
        no_curl: true,
        ..FakeNet::default()
    };
    let r = named("adapter-nocurl", net, Ok(1));
    let got = refusal(spawn(&r));
    assert!(
        matches!(
            got,
            Some(Status::Adapter(status::Adapter::NoDownloader { .. }))
        ),
        "{got:?}"
    );
}
