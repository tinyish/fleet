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

use crate::{Supervisor, Tools};

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
pub(crate) fn resolve(sup: &Supervisor, reference: &str) -> Result<PathBuf, Status> {
    match adapters::parse_ref(reference) {
        Ok(AdapterRef::Path(path)) => speaks(&sup.tools, &path, reference).map(|_| path),
        Ok(AdapterRef::Named(named)) => kept(sup, &named),
        Err(Status::Adapter(status::Adapter::AdapterUnpinned { name, .. })) => {
            Err(Status::Adapter(status::Adapter::AdapterUnpinned {
                latest: latest(sup, &name),
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
fn kept(sup: &Supervisor, named: &Named) -> Result<PathBuf, Status> {
    let (store, tools) = (&sup.store, &sup.tools);
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
    let binary = fetch_binary(sup, named, target, dir, &sha)?;
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
    sup: &Supervisor,
    named: &Named,
    target: &str,
    dir: &Path,
    sha: &str,
) -> Result<Temp, Status> {
    let url = named.binary_url(sup.store.releases(), target);
    let binary = Temp::new(dir, &named.program());
    if download(&sup.tools, &url, &binary.0)? == Fetched::NotFound {
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
fn latest(sup: &Supervisor, name: &str) -> Option<String> {
    let store = &sup.store;
    if store.base.is_some() {
        return None;
    }
    fs::create_dir_all(&store.dir).ok()?;
    let list = Temp::new(&store.dir, "releases.json");
    match sup
        .tools
        .net
        .download(adapters::RELEASE_LIST, &list.0)
        .ok()?
    {
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
