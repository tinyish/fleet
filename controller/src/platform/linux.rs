//! The Linux half of the platform layer.

use super::ServiceFile;
use fleet_core::process::{resolve_on_path, run_bounded};
use std::path::{Path, PathBuf};
use std::process::Command;

/// `fleet` under `XDG_STATE_HOME` where it is set, ahead of any home
/// (shared.locations R5).
pub fn state_dir(xdg_state: Option<&Path>) -> Option<PathBuf> {
    xdg_state.map(|state| state.join("fleet"))
}

/// The directories a child of this controller searches, in order.
///
/// Built from this list and the home passed in, never from the process's own
/// `PATH`: a service-launched process carries a minimal one, and every session
/// started under it inherits that (lessons claude-code D1). There is no `sbin`
/// pair here and no package-manager prefix: a systemd user unit's own
/// environment holds neither, and the agent installs under the home's local
/// bin.
pub fn child_path_dirs(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join(".local").join("bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
        PathBuf::from("/bin"),
    ]
}

/// The process table is `/proc`, not `/bin/ps`. A `/proc` that is not mounted
/// answers `None`: without it this layer cannot tell a gone process from a
/// filesystem it cannot see.
pub fn process_alive(pid: u32) -> Option<bool> {
    if pid == 0 || !Path::new("/proc/self").is_dir() {
        return None;
    }
    Some(Path::new("/proc").join(pid.to_string()).is_dir())
}

// ---- the service manager ----------------------------------------------------

/// The manager this platform's user services are loaded through.
pub const SERVICE_BIN: &str = "systemctl";

/// This platform puts no file-access dialog in front of a directory read, so
/// the gate reads `ok` without probing (lessons claude-code D4).
pub const GRANT_IS_GATED: bool = false;

/// The unit file under the user's own unit directory, which follows
/// `XDG_CONFIG_HOME` where it is set. It always tells the
/// service the machine directory (`FLEET_DIR`), and no `PATH` is written: the
/// controller constructs its children's search path, and one frozen here at
/// install time is what every later session inherits (lessons claude-code D1).
pub fn service_file(
    home: &Path,
    config_home: Option<&Path>,
    label: &str,
    exe: &Path,
    machine_dir: &Path,
) -> ServiceFile {
    // Always, and quoted: see the macOS writer for why the directory is told,
    // and `assignment_value` for why it is quoted.
    let environment = format!(
        "Environment=\"FLEET_DIR={}\"\n",
        assignment_value(machine_dir)
    );
    let text = format!(
        "[Unit]\n\
         Description=the fleet controller\n\
         \n\
         [Service]\n\
         Type=simple\n\
         ExecStart={exe} observe\n\
         Restart=always\n\
         WorkingDirectory={machine}\n\
         {environment}\
         \n\
         [Install]\n\
         WantedBy=default.target\n",
        exe = exe.display(),
        machine = machine_dir.display(),
    );
    ServiceFile {
        path: unit_dir(home, config_home).join(format!("{label}.service")),
        text,
    }
}

/// Where the user's own service manager reads a user's units: under
/// `XDG_CONFIG_HOME` where it is set, under the home's `.config` where it is
/// not (shared.locations R19).
fn unit_dir(home: &Path, config_home: Option<&Path>) -> PathBuf {
    config_home
        .map_or_else(|| home.join(".config"), Path::to_path_buf)
        .join("systemd")
        .join("user")
}

/// A path inside a double-quoted `Environment=` assignment: a backslash and a
/// quote escaped, and a `%` doubled, because the manager reads a lone one as
/// the start of a specifier. Quoted at all because a space would otherwise end
/// the assignment.
fn assignment_value(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

/// This platform's manager collects what the service printed into the user's
/// own journal rather than into a file, so what is named is the command that
/// reads it.
pub fn service_stderr(_machine_dir: &Path) -> String {
    format!(
        "the user journal, read with: journalctl --user -u {}.service",
        super::SERVICE_LABEL
    )
}

/// TWO COMMANDS, in this order: the manager is told to re-read its unit
/// directory, and only then is the unit enabled and started. A unit written
/// after the last read is one the second command would not find.
pub fn load_argv(label: &str, _file: &Path) -> Vec<Vec<String>> {
    vec![
        vec!["--user".to_string(), "daemon-reload".to_string()],
        vec![
            "--user".to_string(),
            "enable".to_string(),
            "--now".to_string(),
            unit(label),
        ],
    ]
}

pub fn unload_argv(label: &str) -> Vec<String> {
    vec!["--user".to_string(), "stop".to_string(), unit(label)]
}

pub fn running_argv(label: &str) -> Vec<String> {
    vec![
        "--user".to_string(),
        "show".to_string(),
        unit(label),
        "--property=MainPID".to_string(),
    ]
}

fn unit(label: &str) -> String {
    format!("{label}.service")
}

/// The pid out of the manager's own report. A unit that is loaded and not
/// running answers zero, which is no pid rather than process 0.
pub fn pid_in(stdout: &str) -> Option<u32> {
    stdout
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("MainPID="))
        .and_then(|pid| pid.trim().parse::<u32>().ok())
        .filter(|pid| *pid != 0)
}

/// LINGERING IS READ AND NEVER SET. A user service stops when the last session
/// of that user ends unless lingering is enabled, and enabling it is a change to
/// the machine's own login policy — so the command is printed for the person and
/// this layer runs nothing.
pub fn after_write(child_path: &str, _file: &Path) -> Vec<String> {
    let Some(bin) = resolve_on_path(child_path, "loginctl") else {
        return vec![format!(
            "lingering was not read: `loginctl` is not on the constructed search path {child_path}"
        )];
    };
    let mut command = Command::new(&bin);
    command.args(["show-user", "--property=Linger"]);
    match run_bounded(command, super::ASIDE_TIMEOUT) {
        Ok(run)
            if run.status.success()
                && String::from_utf8_lossy(&run.stdout).contains("Linger=yes") =>
        {
            vec!["lingering is on: the controller survives a logout".to_string()]
        }
        Ok(run) if run.status.success() => vec![format!(
            "lingering is off, so the controller stops at logout — run this yourself: {} \
             enable-linger",
            bin.display()
        )],
        Ok(run) => vec![format!(
            "lingering could not be read: {}",
            String::from_utf8_lossy(&run.stderr)
                .trim()
                .lines()
                .next()
                .unwrap_or("no reason given")
        )],
        Err(why) => vec![format!("lingering could not be read: {why}")],
    }
}
