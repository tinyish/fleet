//! For an adapter's own tests: temp directories, fake programs on a temp
//! `PATH`, and [`conformance`], the checks every adapter must pass.

pub mod conformance;

use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A directory of a test's own under the system's temp directory, removed
/// when dropped.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    /// A new, empty directory whose name holds `label`.
    ///
    /// # Errors
    ///
    /// When it cannot be made.
    pub fn new(label: &str) -> io::Result<TempDir> {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("fleet-adapter-{label}-{}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir)?;
        Ok(TempDir(dir))
    }

    /// Where it is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// A temp directory of fake programs, put on a `PATH` in place of the real
/// ones.
#[derive(Debug)]
pub struct FakePath(TempDir);

impl FakePath {
    /// An empty one.
    ///
    /// # Errors
    ///
    /// When its directory cannot be made.
    pub fn new() -> io::Result<FakePath> {
        TempDir::new("bin").map(FakePath)
    }

    /// Adds an executable `name` that runs `script` with `/bin/sh`, and
    /// answers its path.
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn program(&self, name: &str, script: &str) -> io::Result<PathBuf> {
        let path = self.0.path().join(name);
        fs::write(&path, format!("#!/bin/sh\n{script}\n"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        Ok(path)
    }

    /// A `PATH` value holding only this directory.
    #[must_use]
    pub fn path_var(&self) -> OsString {
        self.0.path().as_os_str().to_owned()
    }
}
