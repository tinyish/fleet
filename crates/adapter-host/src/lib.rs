//! The adapter host: runs an adapter program once per request, as
//! `<adapter> <verb>`, hands it the request as one JSON object on stdin and
//! reads its answer from stdout (`fleet_core::agent` holds the protocol). A
//! driver, so it may start programs; it depends on nothing in the
//! workspace but `fleet-core`.

use std::io::{self, Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use fleet_core::agent::{self, AdapterFault, Verb};
use fleet_core::tools::AgentAdapters;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// How long an adapter has to answer, unless told otherwise.
pub const TIMEOUT: Duration = Duration::from_secs(20);

/// The most of an answer that is read, in bytes.
const MAX_ANSWER: u64 = 1 << 20;

/// The most of what an adapter says on stderr that is kept, in characters.
const MAX_SAID: usize = 2000;

/// How often a running adapter is checked on.
const TICK: Duration = Duration::from_millis(25);

/// Runs adapter programs, each with a time limit.
#[derive(Debug, Clone, Copy)]
pub struct Host {
    timeout: Duration,
}

impl Default for Host {
    fn default() -> Host {
        Host::with_timeout(TIMEOUT)
    }
}

impl Host {
    /// A host that kills an adapter that has not answered within `timeout`.
    #[must_use]
    pub fn with_timeout(timeout: Duration) -> Host {
        Host { timeout }
    }

    fn run<A: DeserializeOwned>(
        &self,
        adapter: &Path,
        verb: Verb,
        request: &impl Serialize,
    ) -> Result<A, AdapterFault> {
        let mut line = serde_json::to_vec(request).unwrap_or_default();
        line.push(b'\n');
        let mut child = Command::new(adapter)
            .arg(verb.as_str())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| AdapterFault::CouldNotRun {
                error: e.to_string(),
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            // An adapter need not read its request: a closed pipe is fine.
            thread::spawn(move || {
                let _ = stdin.write_all(&line);
            });
        }
        let stdout = drain(child.stdout.take(), MAX_ANSWER);
        let stderr = drain(child.stderr.take(), MAX_ANSWER);
        let Some(status) = self.wait(&mut child) else {
            return Err(AdapterFault::TimedOut {
                after_millis: u64::try_from(self.timeout.as_millis()).unwrap_or(u64::MAX),
            });
        };
        let said = stderr.recv_timeout(TICK * 40).unwrap_or_default();
        if !status.success() {
            return Err(AdapterFault::Exited {
                code: status.code(),
                said: String::from_utf8_lossy(&said)
                    .trim()
                    .chars()
                    .take(MAX_SAID)
                    .collect(),
            });
        }
        let answer = stdout.recv_timeout(TICK * 40).unwrap_or_default();
        serde_json::from_slice(&answer).map_err(|e| AdapterFault::BadAnswer {
            error: e.to_string(),
        })
    }

    /// Waits for `child` to exit, or kills it once the time limit passes.
    fn wait(&self, child: &mut Child) -> Option<std::process::ExitStatus> {
        let until = Instant::now() + self.timeout;
        loop {
            if let Ok(Some(status)) = child.try_wait() {
                return Some(status);
            }
            if Instant::now() >= until {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            thread::sleep(TICK);
        }
    }
}

/// Reads at most `max` bytes of `from` on a thread of its own, so a full
/// pipe never stalls the adapter.
fn drain(from: Option<impl io::Read + Send + 'static>, max: u64) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    if let Some(from) = from {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = from.take(max).read_to_end(&mut bytes);
            let _ = tx.send(bytes);
        });
    }
    rx
}

impl AgentAdapters for Host {
    fn launch(
        &self,
        adapter: &Path,
        request: &agent::Launch,
    ) -> Result<agent::Launched, AdapterFault> {
        self.run(adapter, Verb::Launch, request)
    }

    fn read(&self, adapter: &Path, request: &agent::Read) -> Result<agent::Readings, AdapterFault> {
        self.run(adapter, Verb::Read, request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use fleet_core::agent::{AdapterFault, Launch, Launched, Read, Readings};
    use fleet_core::tools::AgentAdapters;

    /// A temp directory holding one adapter script, deleted at the end.
    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn adapter(name: &str, script: &str) -> (Dir, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("fleet-adapter-host-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).ok();
        let path = dir.join("adapter");
        fs::write(
            &path,
            format!("#!/bin/sh\ncd '{}'\n{script}\n", dir.display()),
        )
        .ok();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).ok();
        (Dir(dir), path)
    }

    fn launch() -> Launch {
        Launch {
            directory: "/c/app".into(),
            model: Some("sonnet".into()),
            prompt: Some("hello; it's me".into()),
        }
    }

    #[test]
    fn answers_a_launch_and_hands_it_the_request() {
        let (dir, path) = adapter(
            "launch",
            r#"echo "$1" > verb; cat > request; echo '{"argv":["/bin/claude","--model","sonnet"]}'"#,
        );
        let answer = Host::default().launch(&path, &launch());
        assert_eq!(
            answer,
            Ok(Launched {
                argv: vec!["/bin/claude".into(), "--model".into(), "sonnet".into()],
                env: std::collections::BTreeMap::new(),
            })
        );
        assert_eq!(
            fs::read_to_string(dir.0.join("verb")).ok().as_deref(),
            Some("launch\n")
        );
        let sent: Option<Launch> = fs::read_to_string(dir.0.join("request"))
            .ok()
            .and_then(|r| serde_json::from_str(&r).ok());
        assert_eq!(sent, Some(launch()));
    }

    #[test]
    fn a_non_zero_exit_is_exited_with_what_it_said() {
        let (_dir, path) = adapter("exit", "echo boom >&2; exit 4");
        assert_eq!(
            Host::default().launch(&path, &launch()),
            Err(AdapterFault::Exited {
                code: Some(4),
                said: "boom".into()
            })
        );
    }

    #[test]
    fn garbage_is_a_bad_answer() {
        let (_dir, path) = adapter("garbage", "echo nope");
        let answer = Host::default().launch(&path, &launch());
        assert!(
            matches!(answer, Err(AdapterFault::BadAnswer { .. })),
            "{answer:?}"
        );
    }

    #[test]
    fn nothing_is_a_bad_answer() {
        let (_dir, path) = adapter("nothing", "exit 0");
        let answer = Host::default().launch(&path, &launch());
        assert!(
            matches!(answer, Err(AdapterFault::BadAnswer { .. })),
            "{answer:?}"
        );
    }

    #[test]
    fn a_hang_is_timed_out_within_the_limit() {
        let (_dir, path) = adapter("hang", "sleep 10");
        let started = Instant::now();
        let answer = Host::with_timeout(Duration::from_millis(300)).launch(&path, &launch());
        assert_eq!(answer, Err(AdapterFault::TimedOut { after_millis: 300 }));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_program_that_is_not_there_cannot_run() {
        let answer = Host::default().launch(&PathBuf::from("/nonexistent/adapter"), &launch());
        assert!(
            matches!(answer, Err(AdapterFault::CouldNotRun { .. })),
            "{answer:?}"
        );
    }

    #[test]
    fn a_program_without_the_exec_bit_cannot_run() {
        let (_dir, path) = adapter("noexec", "echo '{}'");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).ok();
        let answer = Host::default().launch(&path, &launch());
        assert!(
            matches!(answer, Err(AdapterFault::CouldNotRun { .. })),
            "{answer:?}"
        );
    }

    #[test]
    fn an_adapter_that_never_reads_stdin_still_answers() {
        let (_dir, path) = adapter("deaf", r#"echo '{"sessions":[]}'"#);
        let answer = Host::default().read(&path, &Read { sessions: vec![] });
        assert_eq!(answer, Ok(Readings { sessions: vec![] }));
    }
}
