//! The checks every adapter must pass, run against its built program: an
//! adapter's integration test calls [`agent`] with its
//! `CARGO_BIN_EXE_<name>`. Each run is made as tinyfleet's adapter host
//! makes it: `<program> <verb>` in the environment it is checked from, with
//! what [`Adapter::env`] adds, the request on stdin as one line and stdin
//! then closed, stderr kept as the host keeps it, and killed after its time
//! limit.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fmt;
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::Speaks;
use crate::agent::{Launch, Launched, PROTOCOL, Read, Readings, SessionRef, Verb};
use crate::kit::{ANSWERED, FAILED, MAX_JSON, UNKNOWN_VERB};

/// How long each run may take, unless [`Adapter::time_limit`] says.
pub const TIME_LIMIT: Duration = Duration::from_secs(10);

/// The program runs at all.
pub const RUNS: &str = "the program runs";
/// Every run ends within the time limit.
pub const IN_TIME: &str = "answers within the time limit";
/// No verb exits 2, without waiting on stdin.
pub const NO_VERB: &str = "no verb exits 2";
/// An unknown verb exits 2, without waiting on stdin.
pub const UNKNOWN: &str = "an unknown verb exits 2";
/// A request that is not JSON exits 1, says why, and answers nothing.
pub const NOT_JSON: &str =
    "a request that is not JSON exits 1, says why on stderr and writes nothing on stdout";
/// `protocol` answers the kind's protocol number.
pub const PROTOCOL_ANSWERS: &str = "protocol answers the kind's protocol number";
/// Launch answers with exit 0 and a `Launched`.
pub const LAUNCH_ANSWERS: &str = "launch answers with exit 0 and a Launched";
/// Launch's program is an absolute path.
pub const LAUNCH_ABSOLUTE: &str = "launch's program is an absolute path";
/// Read of no sessions answers no readings.
pub const READ_NONE: &str = "read of no sessions answers no readings";
/// Read answers only the sessions asked about, each at most once.
pub const READ_ONLY_ASKED: &str = "read answers only the sessions asked about, each at most once";

/// The most of what an adapter says on stderr that is kept, in characters,
/// once trimmed: adapter-host's `MAX_SAID`.
const MAX_SAID: usize = 2000;

/// How often a running adapter is checked on.
const TICK: Duration = Duration::from_millis(25);

/// A rule an adapter broke, and what it did instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Breach {
    /// The rule, one of this module's constants.
    pub rule: &'static str,
    /// What the adapter did.
    pub saw: String,
}

impl fmt::Display for Breach {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.rule, self.saw)
    }
}

/// An adapter program to check, and the environment it runs in.
#[derive(Debug, Clone)]
pub struct Adapter {
    program: PathBuf,
    env: Vec<(OsString, OsString)>,
    time_limit: Duration,
}

impl Adapter {
    /// The program at `program`, run with the environment it is checked
    /// from, as the adapter host runs it with the supervisor's.
    #[must_use]
    pub fn new(program: impl Into<PathBuf>) -> Adapter {
        Adapter {
            program: program.into(),
            env: Vec::new(),
            time_limit: TIME_LIMIT,
        }
    }

    /// Sets `name` in the environment its runs get.
    #[must_use]
    pub fn env(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Adapter {
        self.env.push((name.into(), value.into()));
        self
    }

    /// How long each run may take.
    #[must_use]
    pub fn time_limit(mut self, limit: Duration) -> Adapter {
        self.time_limit = limit;
        self
    }
}

/// The rules every adapter keeps, and an agent adapter's, checked against
/// `adapter`; `directory` is an existing directory to launch the agent in.
/// No breach means it passes. A program that does not run is one [`RUNS`]
/// breach, and nothing else is checked.
#[must_use]
pub fn agent(adapter: &Adapter, directory: &Path) -> Vec<Breach> {
    let mut check = Check {
        adapter,
        breaches: Vec::new(),
    };
    let _ = check
        .common(&Verb::ALL.map(Verb::as_str))
        .and_then(|()| check.agent_rules(directory));
    check.breaches
}

/// What one run did.
struct Ran {
    code: Option<i32>,
    /// Its raw bytes, parsed as the adapter host parses them.
    stdout: Vec<u8>,
    stderr: String,
}

impl Ran {
    /// The answer, when the run exited 0 with one that reads as a `T`.
    fn answer<T: DeserializeOwned>(&self) -> Option<T> {
        (self.code == Some(i32::from(ANSWERED)))
            .then(|| serde_json::from_slice(&self.stdout).ok())
            .flatten()
    }

    fn exited(&self, code: u8) -> bool {
        self.code == Some(i32::from(code))
    }
}

impl fmt::Display for Ran {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let code = self
            .code
            .map_or_else(|| "a signal".to_owned(), |c| c.to_string());
        let clip = |s: &str| s.trim().chars().take(200).collect::<String>();
        write!(
            f,
            "exited {code}, stdout {:?}, stderr {:?}",
            clip(&String::from_utf8_lossy(&self.stdout)),
            clip(&self.stderr)
        )
    }
}

/// The checks stop: the program does not run.
struct Stop;

/// The checks so far, against one adapter.
struct Check<'a> {
    adapter: &'a Adapter,
    breaches: Vec<Breach>,
}

impl Check<'_> {
    fn breach(&mut self, rule: &'static str, saw: impl fmt::Display) {
        self.breaches.push(Breach {
            rule,
            saw: saw.to_string(),
        });
    }

    /// Runs the adapter once. A run past the time limit is an [`IN_TIME`]
    /// breach and answers nothing, so its own rule is not also reported.
    fn run(&mut self, args: &[&str], stdin: Option<&str>) -> Result<Option<Ran>, Stop> {
        match run(self.adapter, args, stdin) {
            Ok(Some(ran)) => Ok(Some(ran)),
            Ok(None) => {
                let limit = self.adapter.time_limit;
                self.breach(IN_TIME, format!("{args:?} still running after {limit:?}"));
                Ok(None)
            }
            Err(error) => {
                self.breach(RUNS, error);
                Err(Stop)
            }
        }
    }

    fn common(&mut self, verbs: &[&str]) -> Result<(), Stop> {
        for (rule, args) in [(NO_VERB, &[][..]), (UNKNOWN, &["no-such-verb"][..])] {
            if let Some(ran) = self.run(args, None)?
                && !ran.exited(UNKNOWN_VERB)
            {
                self.breach(rule, ran);
            }
        }
        for verb in verbs {
            if let Some(ran) = self.run(&[verb], Some("not json"))?
                && !(ran.exited(FAILED) && !ran.stderr.trim().is_empty() && ran.stdout.is_empty())
            {
                self.breach(NOT_JSON, format!("{verb}: {ran}"));
            }
        }
        Ok(())
    }

    fn agent_rules(&mut self, directory: &Path) -> Result<(), Stop> {
        if let Some(ran) = self.run(&["protocol"], Some("{}"))?
            && ran.answer::<Speaks>() != Some(Speaks { protocol: PROTOCOL })
        {
            self.breach(PROTOCOL_ANSWERS, ran);
        }
        let launch = Launch {
            directory: directory.to_owned(),
            model: Some("conformance-model".into()),
            prompt: Some("-p hello".into()),
        };
        if let Some(ran) = self.run(&["launch"], Some(&json(&launch)))? {
            match ran.answer::<Launched>() {
                None => self.breach(LAUNCH_ANSWERS, ran),
                Some(launched)
                    if !launched
                        .argv
                        .first()
                        .is_some_and(|p| Path::new(p).is_absolute()) =>
                {
                    self.breach(LAUNCH_ABSOLUTE, format!("argv {:?}", launched.argv));
                }
                Some(_) => {}
            }
        }
        if let Some(ran) = self.run(&["read"], Some(r#"{"sessions":[]}"#))?
            && ran
                .answer::<Readings>()
                .is_none_or(|r| !r.sessions.is_empty())
        {
            self.breach(READ_NONE, ran);
        }
        let asked = ["conformance-a", "conformance-b"];
        let read = Read {
            sessions: asked
                .iter()
                .zip(1..)
                .map(|(session, pid)| SessionRef {
                    session: (*session).to_owned(),
                    pid,
                    directory: directory.to_owned(),
                })
                .collect(),
        };
        if let Some(ran) = self.run(&["read"], Some(&json(&read)))? {
            let mut seen = BTreeSet::new();
            let stray = ran.answer::<Readings>().map(|r| {
                r.sessions.into_iter().find(|r| {
                    !asked.contains(&r.session.as_str()) || !seen.insert(r.session.clone())
                })
            });
            match stray {
                None => self.breach(READ_ONLY_ASKED, ran),
                Some(Some(reading)) => {
                    self.breach(
                        READ_ONLY_ASKED,
                        format!("a reading for {:?}", reading.session),
                    );
                }
                Some(None) => {}
            }
        }
        Ok(())
    }
}

fn json(value: &impl Serialize) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// Runs `adapter` once with `args`. With `stdin`, writes it as one line and
/// closes the pipe, as the adapter host does; without, leaves stdin open
/// until the run ends, as a terminal would. None when it ran past the time
/// limit and was killed.
fn run(adapter: &Adapter, args: &[&str], stdin: Option<&str>) -> io::Result<Option<Ran>> {
    let mut child = Command::new(&adapter.program)
        .args(args)
        .envs(adapter.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut held = child.stdin.take();
    if let Some(text) = stdin
        && let Some(mut pipe) = held.take()
    {
        let text = format!("{text}\n").into_bytes();
        thread::spawn(move || {
            let _ = pipe.write_all(&text);
        });
    }
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let until = Instant::now() + adapter.time_limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < until => thread::sleep(TICK),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    drop(held);
    let Some(status) = status else {
        return Ok(None);
    };
    let bytes = |rx: mpsc::Receiver<Vec<u8>>| rx.recv_timeout(TICK * 40).unwrap_or_default();
    Ok(Some(Ran {
        code: status.code(),
        stdout: bytes(stdout),
        stderr: String::from_utf8_lossy(&bytes(stderr))
            .trim()
            .chars()
            .take(MAX_SAID)
            .collect(),
    }))
}

/// Reads at most [`MAX_JSON`] bytes of `from` (adapter-host's `MAX_ANSWER`)
/// on a thread of its own, so a full pipe never stalls the adapter.
fn drain(from: Option<impl io::Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    if let Some(from) = from {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = from.take(MAX_JSON).read_to_end(&mut bytes);
            let _ = tx.send(bytes);
        });
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kit::testing::{FakePath, TempDir};
    use std::io;
    use std::path::Path;
    use std::time::Duration;

    /// A script adapter that keeps every rule: it refuses an unknown verb
    /// at once, refuses a request that is not JSON, launches /bin/echo and
    /// reads no session (leaving one out is allowed: it is unknown).
    const GOOD: &str = r#"
case "$1" in
  launch|read|protocol) ;;
  *) echo "unknown verb $1" >&2; exit 2 ;;
esac
req=$(cat)
case "$req" in
  '{'*) ;;
  *) echo "the request: not JSON" >&2; exit 1 ;;
esac
case "$1" in
  launch) echo '{"argv":["/bin/echo","hi"]}' ;;
  read) echo '{"sessions":[]}' ;;
  protocol) echo '{"protocol":1}' ;;
esac"#;

    /// The rules `script` breaks, each run given the usual time limit: a
    /// script's first start can take over a second while the system checks
    /// a new executable, so only a test about hanging shortens it.
    fn check(script: &str) -> io::Result<Vec<&'static str>> {
        check_within(script, TIME_LIMIT)
    }

    fn check_within(script: &str, limit: Duration) -> io::Result<Vec<&'static str>> {
        let bin = FakePath::new()?;
        let program = bin.program("adapter", script)?;
        let work = TempDir::new("conformance-work")?;
        let adapter = Adapter::new(program).time_limit(limit);
        Ok(agent(&adapter, work.path())
            .iter()
            .map(|b| b.rule)
            .collect())
    }

    #[test]
    fn an_adapter_that_keeps_every_rule_breaches_none() -> io::Result<()> {
        assert_eq!(check(GOOD)?, Vec::<&str>::new());
        Ok(())
    }

    #[test]
    fn an_adapter_that_reads_its_request_as_a_line_breaches_none() -> io::Result<()> {
        let script = GOOD.replace(
            "req=$(cat)",
            r#"IFS= read -r req || { echo "the request: no line" >&2; exit 1; }"#,
        );
        assert_eq!(check(&script)?, Vec::<&str>::new());
        Ok(())
    }

    #[test]
    fn an_adapter_runs_with_the_environment_it_is_checked_from() -> io::Result<()> {
        // cargo sets CARGO_MANIFEST_DIR for a test, as the supervisor's
        // environment holds what it was started with.
        let script = format!(
            r#"[ -n "$CARGO_MANIFEST_DIR" ] || {{ echo "no environment" >&2; exit 1; }}{GOOD}"#
        );
        assert_eq!(check(&script)?, Vec::<&str>::new());
        Ok(())
    }

    #[test]
    fn what_an_adapter_says_is_kept_as_the_host_keeps_it() -> io::Result<()> {
        let bin = FakePath::new()?;
        let program = bin.program("adapter", "printf ' %03000d \n' 0 >&2")?;
        let ran = run(&Adapter::new(program), &[], None)?;
        let said = ran.map(|r| r.stderr).unwrap_or_default();
        assert_eq!(
            (said.chars().count(), said.trim() == said),
            (MAX_SAID, true)
        );
        Ok(())
    }

    #[test]
    fn a_relative_program_breaches_the_absolute_path_rule() -> io::Result<()> {
        let script = GOOD.replace("/bin/echo", "echo");
        assert_eq!(check(&script)?, vec![LAUNCH_ABSOLUTE]);
        Ok(())
    }

    #[test]
    fn a_reading_of_a_session_not_asked_about_breaches() -> io::Result<()> {
        let script = GOOD.replace(
            r#"{"sessions":[]}"#,
            r#"{"sessions":[{"session":"stranger","activity":"idle"}]}"#,
        );
        assert_eq!(check(&script)?, vec![READ_NONE, READ_ONLY_ASKED]);
        Ok(())
    }

    #[test]
    fn an_adapter_that_reads_stdin_before_its_verb_breaches_in_time() -> io::Result<()> {
        let script = GOOD.replace(
            r#"*) echo "unknown verb $1" >&2; exit 2 ;;"#,
            "*) cat >/dev/null; exit 2 ;;",
        );
        // Its first two runs hang by design; the rest start an executable
        // already checked, so a short limit holds for them.
        let limit = Duration::from_millis(1500);
        assert_eq!(check_within(&script, limit)?, vec![IN_TIME, IN_TIME]);
        Ok(())
    }

    #[test]
    fn an_unknown_verb_answered_with_0_breaches() -> io::Result<()> {
        let script = GOOD.replace("exit 2", "exit 0");
        assert_eq!(check(&script)?, vec![NO_VERB, UNKNOWN]);
        Ok(())
    }

    #[test]
    fn garbage_accepted_as_a_request_breaches_not_json() -> io::Result<()> {
        let script = GOOD.replace("exit 1", "echo '{}'; exit 0");
        assert_eq!(check(&script)?, vec![NOT_JSON, NOT_JSON, NOT_JSON]);
        Ok(())
    }

    #[test]
    fn a_launch_that_fails_breaches_launch_answers() -> io::Result<()> {
        let script = GOOD.replace(
            r#"launch) echo '{"argv":["/bin/echo","hi"]}' ;;"#,
            "launch) echo 'no agent here' >&2; exit 1 ;;",
        );
        assert_eq!(check(&script)?, vec![LAUNCH_ANSWERS]);
        Ok(())
    }

    #[test]
    fn an_answer_that_is_not_utf_8_breaches() -> io::Result<()> {
        let script = GOOD.replace(
            r#"launch) echo '{"argv":["/bin/echo","hi"]}' ;;"#,
            r#"launch) printf '{"argv":["/bin/echo","h\377i"]}\n' ;;"#,
        );
        assert_eq!(check(&script)?, vec![LAUNCH_ANSWERS]);
        Ok(())
    }

    #[test]
    fn a_wrong_protocol_number_breaches() -> io::Result<()> {
        let script = GOOD.replace(r#"{"protocol":1}"#, r#"{"protocol":7}"#);
        assert_eq!(check(&script)?, vec![PROTOCOL_ANSWERS]);
        Ok(())
    }

    #[test]
    fn no_protocol_verb_breaches() -> io::Result<()> {
        let script = GOOD.replace("launch|read|protocol)", "launch|read)");
        assert_eq!(check(&script)?, vec![NOT_JSON, PROTOCOL_ANSWERS]);
        Ok(())
    }

    #[test]
    fn a_program_that_is_not_there_is_one_runs_breach() {
        let adapter = Adapter::new("/nonexistent/adapter");
        let breaches: Vec<_> = agent(&adapter, Path::new("/"))
            .iter()
            .map(|b| b.rule)
            .collect();
        assert_eq!(breaches, vec![RUNS]);
    }
}
