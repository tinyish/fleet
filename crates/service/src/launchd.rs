//! launchd, for macOS: a launch agent in the person's GUI session.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use fleet_core::tools::{FileStep, ServiceManager, SupervisorUnit, ToolError};

use crate::{LAUNCHD_LABEL, check_owner, file, render, run};

pub(crate) struct Launchd {
    home: PathBuf,
}

impl Launchd {
    pub(crate) fn new(home: PathBuf) -> Launchd {
        Launchd { home }
    }

    fn plist_path(&self) -> PathBuf {
        self.home
            .join("Library/LaunchAgents")
            .join(format!("{LAUNCHD_LABEL}.plist"))
    }

    fn installed_state_dir(&self) -> Option<PathBuf> {
        let text = fs::read_to_string(self.plist_path()).ok()?;
        render::plist_state_dir(&text)
    }

    fn domain() -> Result<String, ToolError> {
        let out = run("id", &[OsStr::new("-u")])?;
        Ok(format!(
            "gui/{}",
            String::from_utf8_lossy(&out.stdout).trim()
        ))
    }

    fn loaded(target: &str) -> bool {
        run("launchctl", &[OsStr::new("print"), OsStr::new(target)]).is_ok()
    }
}

impl ServiceManager for Launchd {
    fn enable_and_start(&self, unit: &SupervisorUnit) -> Result<(), ToolError> {
        check_owner(self.installed_state_dir(), &unit.state_dir)?;
        let log = unit.state_dir.join("supervisor.log");
        let text = render::plist(unit, &log)?;
        fs::create_dir_all(&unit.state_dir)
            .map_err(|e| file(FileStep::Create, &unit.state_dir, e))?;
        fs::set_permissions(&unit.state_dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| file(FileStep::Secure, &unit.state_dir, e))?;
        let path = self.plist_path();
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| file(FileStep::Create, dir, e))?;
        }
        let tmp = path.with_extension("plist.tmp");
        fs::write(&tmp, text).map_err(|e| file(FileStep::Write, &tmp, e))?;
        fs::rename(&tmp, &path).map_err(|e| file(FileStep::Write, &path, e))?;

        let domain = Launchd::domain()?;
        let target = format!("{domain}/{LAUNCHD_LABEL}");
        if Launchd::loaded(&target) {
            run("launchctl", &[OsStr::new("bootout"), OsStr::new(&target)])?;
        }
        let bootstrap = [
            OsStr::new("bootstrap"),
            OsStr::new(&domain),
            path.as_os_str(),
        ];
        let mut attempt = 1;
        loop {
            match run("launchctl", &bootstrap) {
                Ok(_) => return Ok(()),
                Err(e) if attempt == 3 => return Err(e),
                Err(_) => attempt += 1,
            }
            thread::sleep(Duration::from_millis(300));
        }
    }

    fn stop_and_disable(&self, state_dir: &Path) -> Result<(), ToolError> {
        check_owner(self.installed_state_dir(), state_dir)?;
        let target = format!("{}/{LAUNCHD_LABEL}", Launchd::domain()?);
        if Launchd::loaded(&target) {
            run("launchctl", &[OsStr::new("bootout"), OsStr::new(&target)])?;
        }
        let path = self.plist_path();
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(file(FileStep::Remove, &path, e)),
        }
    }

    fn where_logs_go(&self, state_dir: &Path) -> String {
        state_dir.join("supervisor.log").display().to_string()
    }
}
