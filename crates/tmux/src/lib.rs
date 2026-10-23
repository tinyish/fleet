//! The tmux driver: runs the person's `tmux` on tinyfleet's own server,
//! whose socket is a file in the state directory, never the person's
//! server and never their `~/.tmux.conf`. A driver, so it may start
//! programs; it depends on nothing in the workspace but `fleet-core`.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use fleet_core::agent::Launched;
use fleet_core::tools::{FileStep, Multiplexer, Pane, ToolError};

/// The socket's file name in the state directory.
pub const SOCKET: &str = "tmux.sock";

/// What tmux says when its server is not running: no sessions, not a
/// failure.
const NO_SERVER: [&str; 2] = ["no server running", "error connecting to"];

/// tmux, on the server at one socket.
#[derive(Debug, Clone)]
pub struct Tmux {
    program: String,
    socket: PathBuf,
}

impl Tmux {
    /// tmux on the server whose socket is `socket`.
    #[must_use]
    pub fn new(socket: PathBuf) -> Tmux {
        Tmux::with_program("tmux", socket)
    }

    /// `program`, run as tmux, on the server whose socket is `socket`.
    #[must_use]
    pub fn with_program(program: &str, socket: PathBuf) -> Tmux {
        Tmux {
            program: program.to_owned(),
            socket,
        }
    }

    /// Runs tmux with `args`; a failure names the tmux command, `doing`,
    /// and never the arguments, which hold the agent's environment and
    /// prompt.
    fn run(&self, doing: &str, args: &[OsString]) -> Result<Output, ToolError> {
        let out = Command::new(&self.program)
            .arg("-S")
            .arg(&self.socket)
            .args(["-f", "/dev/null"])
            .args(args)
            .output()
            .map_err(|error| ToolError::CouldNotRun {
                program: "tmux".into(),
                error,
            })?;
        if out.status.success() {
            return Ok(out);
        }
        Err(ToolError::Failed {
            command: format!("tmux {doing}"),
            said: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        })
    }
}

impl Multiplexer for Tmux {
    fn launch(&self, session: &str, directory: &Path, program: &Launched) -> Result<(), ToolError> {
        if !directory.is_dir() {
            return Err(ToolError::File {
                doing: FileStep::Read,
                path: directory.to_path_buf(),
                error: io::Error::from(io::ErrorKind::NotFound),
            });
        }
        let mut args: Vec<OsString> = [
            "start-server",
            ";",
            "set-option",
            "-g",
            "remain-on-exit",
            "on",
            ";",
            "new-session",
            "-d",
            "-s",
            session,
            "-c",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        // `-c` expands tmux formats, which start with `#`.
        args.push(escaped(&directory.display().to_string().replace('#', "##")).into());
        args.push("--".into());
        args.push("/usr/bin/env".into());
        args.push("-i".into());
        args.extend(
            environment(program)
                .iter()
                .map(|e| OsString::from(escaped(e))),
        );
        args.extend(program.argv.iter().map(|a| OsString::from(escaped(a))));
        self.run("new-session", &args).map(|_| ())
    }

    fn panes(&self) -> Result<Vec<Pane>, ToolError> {
        let args: Vec<OsString> = [
            "list-panes",
            "-a",
            "-F",
            "#{session_name}\t#{pane_pid}\t#{pane_dead}",
        ]
        .iter()
        .map(OsString::from)
        .collect();
        match self.run("list-panes", &args) {
            Ok(out) => Ok(String::from_utf8_lossy(&out.stdout)
                .lines()
                .filter_map(pane)
                .collect()),
            Err(ToolError::Failed { said, .. }) if no_server(&said) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    fn stop(&self, session: &str) -> Result<(), ToolError> {
        let target = format!("={session}");
        let args: Vec<OsString> = ["kill-session", "-t", &target]
            .iter()
            .map(OsString::from)
            .collect();
        match self.run("kill-session", &args) {
            Ok(_) => Ok(()),
            Err(ToolError::Failed { said, .. })
                if no_server(&said) || said.contains("can't find session") =>
            {
                Ok(())
            }
            Err(e) => Err(e),
        }
    }
}

/// What the agent keeps of the supervisor's environment: who it runs as
/// (Claude Code finds its login in the macOS Keychain by `USER`), where,
/// and with what shell and language.
const KEPT: [&str; 7] = ["HOME", "USER", "LOGNAME", "PATH", "SHELL", "TMPDIR", "LANG"];

/// The agent's whole environment, as `NAME=value` arguments to `env -i`:
/// the supervisor's [`KEPT`] variables, tmux's `TERM`, then what the
/// adapter asks for, which wins.
fn environment(program: &Launched) -> Vec<String> {
    let mut env = Vec::new();
    for name in KEPT {
        if let Some(value) = std::env::var_os(name) {
            env.push(format!("{name}={}", value.to_string_lossy()));
        }
    }
    env.push("TERM=tmux-256color".to_owned());
    env.extend(program.env.iter().map(|(k, v)| format!("{k}={v}")));
    env
}

/// tmux reads an argument ending in `;` as a command separator, whichever
/// argument it is: the directory, an environment entry or the program's.
fn escaped(arg: &str) -> String {
    match arg.strip_suffix(';') {
        Some(head) => format!(r"{head}\;"),
        None => arg.to_owned(),
    }
}

fn no_server(said: &str) -> bool {
    NO_SERVER.iter().any(|s| said.contains(s))
}

/// One line of `list-panes`, or none when it does not parse.
fn pane(line: &str) -> Option<Pane> {
    let mut fields = line.split('\t');
    let session = fields.next()?.to_owned();
    let pid = fields.next()?.parse().ok()?;
    let dead = fields.next()? == "1";
    Some(Pane { session, pid, dead })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::thread;
    use std::time::{Duration, Instant};

    use fleet_core::agent::Launched;
    use fleet_core::tools::{Multiplexer, Pane, ToolError};

    /// A tmux server of the test's own, on a socket in a temp directory,
    /// killed and deleted when the test ends.
    struct Server {
        tmux: Tmux,
        dir: PathBuf,
        socket: PathBuf,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = Command::new("tmux")
                .arg("-S")
                .arg(&self.socket)
                .arg("kill-server")
                .output();
            let _ = fs::remove_dir_all(&self.dir);
        }
    }

    /// A server for `name`, or none when tmux is not on the `PATH`.
    fn server(name: &str) -> Option<Server> {
        if Command::new("tmux").arg("-V").output().is_err() {
            let _ = std::io::Write::write_all(
                &mut std::io::stderr(),
                b"tmux is not on the PATH; skipping\n",
            );
            return None;
        }
        let dir = std::env::temp_dir().join(format!("fleet-tmux-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).ok()?;
        let dir = fs::canonicalize(&dir).ok()?;
        let socket = dir.join(SOCKET);
        Some(Server {
            tmux: Tmux::new(socket.clone()),
            dir,
            socket,
        })
    }

    fn sh(script: &str) -> Launched {
        Launched {
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            env: BTreeMap::new(),
        }
    }

    /// Waits up to five seconds for `done`.
    fn wait_for(what: &str, done: impl Fn() -> bool) {
        let until = Instant::now() + Duration::from_secs(5);
        while !done() && Instant::now() < until {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(done(), "timed out waiting for {what}");
    }

    fn written(path: &std::path::Path) -> bool {
        fs::metadata(path).is_ok_and(|m| m.len() > 0)
    }

    #[test]
    fn launches_lists_and_stops_a_session() {
        let Some(s) = server("cycle") else { return };
        let dir = s.dir.join("work #{session_name} #1 space");
        fs::create_dir_all(&dir).ok();
        assert!(
            s.tmux
                .launch("s1", &dir, &sh("pwd > out; sleep 30"))
                .is_ok()
        );
        wait_for("pwd", || written(&dir.join("out")));
        let pwd = fs::read_to_string(dir.join("out")).unwrap_or_default();
        assert_eq!(pwd.trim(), dir.display().to_string());
        let panes = s.tmux.panes().unwrap_or_default();
        assert!(
            matches!(panes.as_slice(), [Pane { session, dead: false, pid }] if session == "s1" && *pid > 0),
            "{panes:?}"
        );
        assert!(s.tmux.stop("s1").is_ok());
        assert_eq!(s.tmux.panes().ok(), Some(vec![]));
        assert!(
            s.tmux.stop("s1").is_ok(),
            "stopping a session that is not there is fine"
        );
    }

    #[test]
    fn arguments_reach_the_program_unchanged() {
        let Some(s) = server("args") else { return };
        let args = ["a;", "b c", "say \"hi\"", "line\nbreak", "#{pane_pid}", ";"];
        let mut program = sh(r#"for a in "$@"; do printf '%s\0' "$a"; done > out; sleep 30"#);
        program.argv.push("sh".into());
        program.argv.extend(args.iter().map(|a| (*a).to_owned()));
        assert!(s.tmux.launch("s2", &s.dir, &program).is_ok());
        wait_for("the arguments", || written(&s.dir.join("out")));
        let out = fs::read(s.dir.join("out")).unwrap_or_default();
        let got: Vec<String> = out
            .split(|b| *b == 0)
            .filter(|a| !a.is_empty())
            .map(|a| String::from_utf8_lossy(a).into_owned())
            .collect();
        assert_eq!(got, args);
    }

    #[test]
    fn a_directory_or_an_env_value_ending_in_a_semicolon_reaches_the_program() {
        let Some(s) = server("semi") else { return };
        let dir = s.dir.join("odd;");
        fs::create_dir_all(&dir).ok();
        let mut program = sh(r#"printf '%s|%s' "$(pwd)" "$ODD" > out; sleep 30"#);
        program.env.insert("ODD".into(), "value;".into());
        let launched = s.tmux.launch("s6", &dir, &program);
        assert!(launched.is_ok(), "{launched:?}");
        wait_for("the directory", || written(&dir.join("out")));
        assert_eq!(
            fs::read_to_string(dir.join("out")).ok(),
            Some(format!("{}|value;", dir.display()))
        );
    }

    #[test]
    fn a_failure_names_the_tmux_command_and_nothing_of_the_agents() {
        let Some(s) = server("dup") else { return };
        let mut program = sh("sleep 30");
        program.env.insert("TOKEN".into(), "secret".into());
        assert!(s.tmux.launch("s7", &s.dir, &program).is_ok());
        let again = s.tmux.launch("s7", &s.dir, &program);
        assert!(
            matches!(&again, Err(ToolError::Failed { command, said })
                if command == "tmux new-session" && said.contains("duplicate")),
            "{again:?}"
        );
    }

    #[test]
    fn a_tmux_that_is_not_installed_cannot_run() {
        let dir = std::env::temp_dir();
        let tmux = Tmux::with_program("fleet-no-such-tmux", dir.join(SOCKET));
        assert!(
            matches!(tmux.panes(), Err(ToolError::CouldNotRun { program, .. }) if program == "tmux"),
            "{:?}",
            tmux.panes()
        );
        assert!(matches!(
            tmux.launch("s8", &dir, &sh("true")),
            Err(ToolError::CouldNotRun { .. })
        ));
    }

    #[test]
    fn the_agent_keeps_who_it_runs_as() {
        let Some(s) = server("user") else { return };
        // Claude Code finds its login in the macOS Keychain by USER.
        let program =
            sh(r#"printf '%s|%s|%s|%s' "$USER" "$LOGNAME" "$SHELL" "$TMPDIR" > out; sleep 30"#);
        assert!(s.tmux.launch("s9", &s.dir, &program).is_ok());
        wait_for("the environment", || written(&s.dir.join("out")));
        let want: Vec<String> = ["USER", "LOGNAME", "SHELL", "TMPDIR"]
            .iter()
            .map(|n| std::env::var(n).unwrap_or_default())
            .collect();
        assert_eq!(
            fs::read_to_string(s.dir.join("out")).ok(),
            Some(want.join("|"))
        );
    }

    #[test]
    fn an_exited_agent_leaves_a_dead_pane() {
        let Some(s) = server("dead") else { return };
        assert!(s.tmux.launch("s3", &s.dir, &sh("exit 3")).is_ok());
        wait_for("a dead pane", || {
            s.tmux.panes().unwrap_or_default().iter().any(|p| p.dead)
        });
    }

    #[test]
    fn no_server_means_no_panes() {
        let Some(s) = server("none") else { return };
        assert_eq!(s.tmux.panes().ok(), Some(vec![]));
    }

    #[test]
    fn a_missing_directory_is_refused_before_tmux_runs() {
        let Some(s) = server("missing") else { return };
        let result = s.tmux.launch("s4", &s.dir.join("nope"), &sh("true"));
        assert!(matches!(result, Err(ToolError::File { .. })), "{result:?}");
        assert!(!s.socket.exists(), "tmux never ran");
    }

    #[test]
    fn the_adapter_env_reaches_the_program_and_nothing_else_does() {
        let Some(s) = server("env") else { return };
        // cargo sets CARGO_MANIFEST_DIR for the test; the agent must not inherit it.
        let mut program = sh(
            r#"printf '%s|%s|%s' "$FROM_ADAPTER" "${CARGO_MANIFEST_DIR:-unset}" "$TERM" > out; sleep 30"#,
        );
        program.env.insert("FROM_ADAPTER".into(), "yes".into());
        assert!(s.tmux.launch("s5", &s.dir, &program).is_ok());
        wait_for("the environment", || written(&s.dir.join("out")));
        assert_eq!(
            fs::read_to_string(s.dir.join("out")).ok().as_deref(),
            Some("yes|unset|tmux-256color")
        );
    }
}
