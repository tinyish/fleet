//! What an agent's adapter needs: the [`AgentAdapter`] trait and its run
//! loop, arguments that are never read as the agent's own flags, the
//! per-session status files many agents keep (one per process), and
//! readings that name only the sessions asked about.

use std::path::Path;
use std::process::ExitCode;

use serde::de::DeserializeOwned;

use super::{Error, JsonFile};
use crate::agent::{Activity, Launch, Launched, Read, Reading, Readings, Reason, SessionRef, Verb};

/// An agent's adapter: the two verbs it answers itself; the kit answers
/// `protocol` for it.
pub trait AgentAdapter {
    /// What to run to start the agent. It runs nothing itself: tinyfleet
    /// starts the session.
    ///
    /// # Errors
    ///
    /// When the agent cannot be started here, such as its program missing.
    fn launch(&self, request: &Launch) -> Result<Launched, Error>;

    /// What each session's agent is doing; build the answer with
    /// [`readings`].
    ///
    /// # Errors
    ///
    /// When no session can be read, such as the agent's records having no
    /// home.
    fn read(&self, request: &Read) -> Result<Readings, Error>;
}

/// Runs one request against `adapter`: an agent adapter's whole `main`.
#[must_use]
pub fn run(adapter: &impl AgentAdapter) -> ExitCode {
    super::run(&Verb::ALL.map(Verb::as_str), |verb, request| {
        answer(adapter, verb, request)
    })
}

/// `adapter`'s answer to `verb` with `request`, as [`super::run_on`] takes
/// it.
///
/// # Errors
///
/// When the request does not read, the adapter fails, or `verb` is not an
/// agent verb.
pub fn answer(adapter: &impl AgentAdapter, verb: &str, request: &str) -> Result<String, Error> {
    match Verb::parse(verb) {
        Some(Verb::Launch) => super::handle(request, |r: Launch| adapter.launch(&r)),
        Some(Verb::Read) => super::handle(request, |r: Read| adapter.read(&r)),
        Some(Verb::Protocol) => super::handle(request, |_: serde::de::IgnoredAny| {
            Ok(crate::Speaks {
                protocol: crate::agent::PROTOCOL,
            })
        }),
        None => Err(Error::new(format!("unknown verb \"{verb}\""))),
    }
}

/// The program and arguments of a [`Launched`], built so that no value is
/// ever read as one of the agent's own flags.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Argv(Vec<String>);

impl Argv {
    /// Starts with `program`, which tinyfleet runs by its path.
    ///
    /// # Errors
    ///
    /// When `program` is not absolute, or not UTF-8.
    pub fn program(program: &Path) -> Result<Argv, Error> {
        if !program.is_absolute() {
            return Err(Error::new(format!(
                "{} is not an absolute path",
                program.display()
            )));
        }
        let program = program
            .to_str()
            .ok_or_else(|| Error::new(format!("{} is not UTF-8", program.display())))?;
        Ok(Argv(vec![program.to_owned()]))
    }

    /// Adds `<flag>=<value>` as one argument, so a value starting with `-`
    /// stays the flag's; adds nothing without a value.
    #[must_use]
    pub fn option(mut self, flag: &str, value: Option<&str>) -> Argv {
        if let Some(value) = value {
            self.0.push(format!("{flag}={value}"));
        }
        self
    }

    /// Ends the arguments: `--`, then `value`, so it is read as an argument
    /// whatever it starts with. Without a value, adds nothing.
    #[must_use]
    pub fn last(mut self, value: Option<&str>) -> Vec<String> {
        if let Some(value) = value {
            self.0.push("--".to_owned());
            self.0.push(value.to_owned());
        }
        self.0
    }
}

/// What an agent's status file for one process, `<dir>/<pid>.json`, says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusFile<T> {
    /// There is no file for the pid: the agent has not written it yet.
    NotYet,
    /// The file names another process: one that ended, its pid since
    /// reused.
    Stale,
    /// Something is there that does not read (see [`JsonFile::Broken`]).
    Broken,
    /// The file, read.
    Read(T),
}

/// Reads the status file for `pid` in `dir`; `pid_of` says which process
/// a file is about.
#[must_use]
pub fn status_file<T: DeserializeOwned>(
    dir: &Path,
    pid: u32,
    pid_of: impl Fn(&T) -> u32,
) -> StatusFile<T> {
    match super::read_json::<T>(&dir.join(format!("{pid}.json"))) {
        JsonFile::Missing => StatusFile::NotYet,
        JsonFile::Broken => StatusFile::Broken,
        JsonFile::Read(file) if pid_of(&file) != pid => StatusFile::Stale,
        JsonFile::Read(file) => StatusFile::Read(file),
    }
}

/// A reading of each session asked about, in order, from what `read` says
/// of it. The session ids are the request's, so a reading never names a
/// session nobody asked about.
#[must_use]
pub fn readings(
    request: &Read,
    mut read: impl FnMut(&SessionRef) -> (Activity, Option<Reason>),
) -> Readings {
    Readings {
        sessions: request
            .sessions
            .iter()
            .map(|session| {
                let (activity, reason) = read(session);
                Reading {
                    session: session.session.clone(),
                    activity,
                    reason,
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::ffi::OsString;
    use std::fs;
    use std::io;
    use std::path::Path;

    use serde::Deserialize;

    use crate::agent::{Activity, Launch, Launched, Read, Readings, Reason, SessionRef, Verb};
    use crate::kit::testing::TempDir;
    use crate::kit::{ANSWERED, Error, UNKNOWN_VERB, run_on};

    /// An adapter that launches /bin/agent and reads every session idle.
    struct Fake;

    impl AgentAdapter for Fake {
        fn launch(&self, request: &Launch) -> Result<Launched, Error> {
            Ok(Launched {
                argv: Argv::program(Path::new("/bin/agent"))?
                    .option("--model", request.model.as_deref())
                    .last(request.prompt.as_deref()),
                env: BTreeMap::new(),
            })
        }

        fn read(&self, request: &Read) -> Result<Readings, Error> {
            Ok(readings(request, |_| (Activity::Idle, None)))
        }
    }

    fn ask(verb: &str, request: &str) -> (u8, String) {
        let mut out = Vec::new();
        let code = run_on(
            [OsString::from(verb)],
            request.as_bytes(),
            &mut out,
            &mut Vec::new(),
            &Verb::ALL.map(Verb::as_str),
            |v, r| answer(&Fake, v, r),
        );
        (code, String::from_utf8_lossy(&out).into_owned())
    }

    #[test]
    fn the_agent_run_loop_answers_launch_and_read() {
        assert_eq!(
            ask(
                "launch",
                r#"{"directory":"/c/app","model":"m","prompt":"hi"}"#
            ),
            (
                ANSWERED,
                "{\"argv\":[\"/bin/agent\",\"--model=m\",\"--\",\"hi\"]}\n".into()
            )
        );
        assert_eq!(
            ask(
                "read",
                r#"{"sessions":[{"session":"s1","pid":7,"directory":"/c/app"}]}"#
            ),
            (
                ANSWERED,
                "{\"sessions\":[{\"session\":\"s1\",\"activity\":\"idle\"}]}\n".into()
            )
        );
        assert_eq!(ask("stop", "{}").0, UNKNOWN_VERB);
    }

    #[test]
    fn the_kit_answers_protocol_for_any_agent_adapter() {
        assert_eq!(
            ask("protocol", "{}"),
            (ANSWERED, "{\"protocol\":1}\n".into())
        );
    }

    #[test]
    fn protocol_needs_a_json_request_like_any_verb() {
        assert_eq!(ask("protocol", "").0, crate::kit::FAILED);
        assert_eq!(ask("protocol", "not json").0, crate::kit::FAILED);
    }

    #[test]
    fn answer_refuses_a_verb_that_is_not_an_agent_verb() {
        assert!(answer(&Fake, "stop", "{}").is_err());
    }

    #[test]
    fn argv_keeps_every_value_one_argument() {
        let program = || Argv::program(Path::new("/bin/agent"));
        assert_eq!(
            program().map(|a| a
                .option("--model", Some("-x=y z"))
                .option("--skip", None)
                .last(Some("--help\nme"))),
            Ok(vec![
                "/bin/agent".into(),
                "--model=-x=y z".into(),
                "--".into(),
                "--help\nme".into()
            ])
        );
        assert_eq!(
            program().map(|a| a.option("--model", Some("")).last(None)),
            Ok(vec!["/bin/agent".into(), "--model=".into()])
        );
        assert_eq!(
            program().map(|a| a.last(Some(""))),
            Ok(vec!["/bin/agent".into(), "--".into(), String::new()])
        );
    }

    #[test]
    fn argv_refuses_a_relative_or_non_utf8_program() {
        use std::os::unix::ffi::OsStrExt;
        assert!(Argv::program(Path::new("bin/agent")).is_err());
        let odd = Path::new(std::ffi::OsStr::from_bytes(b"/bin/\xffagent"));
        assert!(Argv::program(odd).is_err());
    }

    #[derive(Deserialize, Debug, PartialEq, Eq)]
    struct Status {
        pid: u32,
    }

    #[test]
    fn status_file_tells_not_yet_stale_broken_and_read_apart() -> io::Result<()> {
        let dir = TempDir::new("status")?;
        let file = dir.path().join("7.json");
        let read = || status_file(dir.path(), 7, |s: &Status| s.pid);
        assert_eq!(read(), StatusFile::NotYet);
        fs::write(&file, r#"{"pid":9}"#)?;
        assert_eq!(read(), StatusFile::Stale);
        fs::write(&file, r#"{"pid":"#)?;
        assert_eq!(read(), StatusFile::Broken);
        fs::write(&file, r#"{"pid":7}"#)?;
        assert_eq!(read(), StatusFile::Read(Status { pid: 7 }));
        fs::remove_file(&file)?;
        fs::create_dir(&file)?;
        assert_eq!(read(), StatusFile::Broken);
        Ok(())
    }

    #[test]
    fn readings_names_each_session_asked_about_in_order() {
        let session = |id: &str, pid| SessionRef {
            session: id.into(),
            pid,
            directory: "/c/app".into(),
        };
        let request = Read {
            sessions: vec![session("s1", 1), session("s2", 2)],
        };
        let r = readings(&request, |s| {
            if s.pid == 1 {
                (Activity::NeedsYou, Some(Reason::Permission))
            } else {
                (Activity::Working, None)
            }
        });
        let got: Vec<_> = r
            .sessions
            .iter()
            .map(|x| (x.session.as_str(), x.activity, x.reason))
            .collect();
        assert_eq!(
            got,
            vec![
                ("s1", Activity::NeedsYou, Some(Reason::Permission)),
                ("s2", Activity::Working, None)
            ]
        );
    }
}
