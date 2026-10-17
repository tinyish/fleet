//! systemd, for Linux: a user unit.

use std::ffi::OsStr;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use fleet_core::tools::{FileStep, ServiceManager, SupervisorUnit, ToolError};

use crate::{SYSTEMD_UNIT, check_owner, file, render, run};

pub(crate) struct Systemd {
    units_dir: PathBuf,
}

impl Systemd {
    pub(crate) fn new(units_dir: PathBuf) -> Systemd {
        Systemd { units_dir }
    }

    fn unit_path(&self) -> PathBuf {
        self.units_dir.join(SYSTEMD_UNIT)
    }
}

fn systemctl(args: &[&str]) -> Result<(), ToolError> {
    let mut all: Vec<&OsStr> = vec![OsStr::new("--user")];
    all.extend(args.iter().map(OsStr::new));
    run("systemctl", &all).map(|_| ())
}

impl ServiceManager for Systemd {
    fn enable_and_start(&self, unit: &SupervisorUnit) -> Result<(), ToolError> {
        check_owner(self.installed_state_dir(), &unit.state_dir)?;
        let text = render::systemd_unit(unit)?;
        fs::create_dir_all(&unit.state_dir)
            .map_err(|e| file(FileStep::Create, &unit.state_dir, e))?;
        fs::set_permissions(&unit.state_dir, fs::Permissions::from_mode(0o700))
            .map_err(|e| file(FileStep::Secure, &unit.state_dir, e))?;
        fs::create_dir_all(&self.units_dir)
            .map_err(|e| file(FileStep::Create, &self.units_dir, e))?;
        let path = self.unit_path();
        let tmp = path.with_extension("service.tmp");
        fs::write(&tmp, text).map_err(|e| file(FileStep::Write, &tmp, e))?;
        fs::rename(&tmp, &path).map_err(|e| file(FileStep::Write, &path, e))?;
        systemctl(&["daemon-reload"])?;
        systemctl(&["enable", SYSTEMD_UNIT])?;
        systemctl(&["restart", SYSTEMD_UNIT])
    }

    fn stop_and_disable(&self, state_dir: &Path) -> Result<(), ToolError> {
        check_owner(self.installed_state_dir(), state_dir)?;
        let path = self.unit_path();
        if !path.exists() {
            return Ok(());
        }
        systemctl(&["disable", "--now", SYSTEMD_UNIT])?;
        fs::remove_file(&path).map_err(|e| file(FileStep::Remove, &path, e))?;
        systemctl(&["daemon-reload"])
    }

    fn running(&self, state_dir: &Path) -> bool {
        self.installed_state_dir().as_deref() == Some(state_dir)
            && systemctl(&["is-active", "--quiet", SYSTEMD_UNIT]).is_ok()
    }

    fn where_logs_go(&self, _state_dir: &Path) -> String {
        format!("journalctl --user -u {SYSTEMD_UNIT}")
    }

    fn name(&self) -> &'static str {
        "systemd"
    }

    fn answers(&self) -> Result<(), ToolError> {
        systemctl(&["show-environment"])
    }

    fn installed_state_dir(&self) -> Option<PathBuf> {
        let text = fs::read_to_string(self.unit_path()).ok()?;
        render::systemd_state_dir(&text)
    }
}
