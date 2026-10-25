//! What anyone writing an adapter in Rust needs, whatever it adapts: the
//! run loop ([`run`]), the error a verb fails with, and helpers for finding
//! a program, a config directory and a small JSON file. [`testing`] is for
//! an adapter's own tests.

pub mod agent;
pub mod testing;

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::fs;
use std::io::{self, Read as _};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// The exit code of an adapter that answered.
pub const ANSWERED: u8 = 0;
/// The exit code of an adapter that could not answer; why is on stderr.
pub const FAILED: u8 = 1;
/// The exit code of an adapter asked for a verb it does not answer.
pub const UNKNOWN_VERB: u8 = 2;

/// The most of a request, or of a JSON file, that is read, in bytes.
pub const MAX_JSON: u64 = 1 << 20;

/// Why an adapter could not answer: one line, written to stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(String);

impl Error {
    /// An error saying `why`.
    #[must_use]
    pub fn new(why: impl Into<String>) -> Error {
        Error(why.into())
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Runs one request as tinyfleet makes it: the verb is the first argument,
/// the request is stdin. `verbs` lists the verbs this adapter answers; for
/// one of them, `answer` turns the request's text into the answer's (see
/// [`handle`]). An adapter's whole `main`, whatever it adapts.
#[must_use]
pub fn run(verbs: &[&str], answer: impl FnOnce(&str, &str) -> Result<String, Error>) -> ExitCode {
    let code = run_on(
        std::env::args_os().skip(1),
        io::stdin().lock(),
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
        verbs,
        answer,
    );
    ExitCode::from(code)
}

/// [`run`], on the arguments (the program's name left out) and the streams
/// given, answering its exit code. A verb not in `verbs` is refused before
/// stdin is read, so a person who runs the adapter at a terminal is never
/// left waiting.
pub fn run_on(
    args: impl IntoIterator<Item = OsString>,
    stdin: impl io::Read,
    stdout: &mut impl io::Write,
    stderr: &mut impl io::Write,
    verbs: &[&str],
    answer: impl FnOnce(&str, &str) -> Result<String, Error>,
) -> u8 {
    let verb = args.into_iter().next().unwrap_or_default();
    let Some(verb) = verb.to_str().filter(|v| verbs.contains(v)) else {
        let _ = writeln!(
            stderr,
            "unknown verb \"{}\"; this adapter answers {}",
            verb.display(),
            verbs.join(", ")
        );
        return UNKNOWN_VERB;
    };
    let answered = read_request(stdin)
        .and_then(|request| answer(verb, &request))
        .and_then(|mut line| {
            line.push('\n');
            stdout
                .write_all(line.as_bytes())
                .and_then(|()| stdout.flush())
                .map_err(|e| Error::new(format!("writing the answer: {e}")))
        });
    match answered {
        Ok(()) => ANSWERED,
        Err(why) => {
            let _ = writeln!(stderr, "{why}");
            FAILED
        }
    }
}

fn read_request(stdin: impl io::Read) -> Result<String, Error> {
    let mut text = String::new();
    stdin
        .take(MAX_JSON + 1)
        .read_to_string(&mut text)
        .map_err(|e| Error::new(format!("reading the request: {e}")))?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_JSON {
        return Err(Error::new(format!(
            "the request is longer than {MAX_JSON} bytes"
        )));
    }
    Ok(text)
}

/// One verb's work: reads `request` as a `Q`, hands it to `handler`, and
/// answers what it answers as one line of JSON.
///
/// # Errors
///
/// When the request does not read as a `Q`, or `handler` fails.
pub fn handle<Q: DeserializeOwned, A: Serialize>(
    request: &str,
    handler: impl FnOnce(Q) -> Result<A, Error>,
) -> Result<String, Error> {
    let request =
        serde_json::from_str(request).map_err(|e| Error::new(format!("the request: {e}")))?;
    let answer = handler(request)?;
    serde_json::to_string(&answer).map_err(|e| Error::new(format!("the answer: {e}")))
}

/// The first executable file called `name` in the absolute directories of
/// `path`, a `PATH` value. Relative and empty entries are skipped: what
/// they find would depend on where the adapter happened to run.
#[must_use]
pub fn find_program(name: &str, path: Option<&OsStr>) -> Option<PathBuf> {
    path.map(std::env::split_paths)
        .into_iter()
        .flatten()
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// A tool's config directory: the variable `var` when it holds an
/// absolute path, otherwise `home_relative` under `HOME` when that does.
/// A relative value is passed over, as [`find_program`] passes over a
/// relative `PATH` entry: the adapter and the agent would each read it
/// from where they happen to run. `env` reads a variable; an adapter hands
/// in `|name| std::env::var_os(name)`.
#[must_use]
pub fn config_dir(
    var: &str,
    home_relative: &str,
    env: impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    let absolute = |value: OsString| Some(PathBuf::from(value)).filter(|p| p.is_absolute());
    env(var).and_then(absolute).or_else(|| {
        env("HOME")
            .and_then(absolute)
            .map(|home| home.join(home_relative))
    })
}

/// What reading a small JSON file found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonFile<T> {
    /// Nothing is there.
    Missing,
    /// Something is there that is not a regular file, is larger than
    /// [`MAX_JSON`], cannot be read, or is not a `T`.
    Broken,
    /// The file, read.
    Read(T),
}

/// Reads the JSON file at `path` as a `T`, reading at most [`MAX_JSON`]
/// bytes; anything there but a regular file is broken, never opened.
#[must_use]
pub fn read_json<T: DeserializeOwned>(path: &Path) -> JsonFile<T> {
    match fs::metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return JsonFile::Missing,
        Ok(m) if m.is_file() && m.len() <= MAX_JSON => {}
        _ => return JsonFile::Broken,
    }
    let mut bytes = Vec::new();
    let read = fs::File::open(path).and_then(|f| f.take(MAX_JSON + 1).read_to_end(&mut bytes));
    match read {
        Err(e) if e.kind() == io::ErrorKind::NotFound => JsonFile::Missing,
        Ok(n) if u64::try_from(n).unwrap_or(u64::MAX) <= MAX_JSON => {
            serde_json::from_slice(&bytes).map_or(JsonFile::Broken, JsonFile::Read)
        }
        _ => JsonFile::Broken,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::ffi::OsString;
    use std::fs;
    use std::io;
    use std::os::unix::fs::PermissionsExt;

    use crate::kit::testing::TempDir;

    /// A stdin that records whether it was read.
    struct Watched<'a>(&'a Cell<bool>, &'a [u8]);

    impl io::Read for Watched<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0.set(true);
            io::Read::read(&mut self.1, buf)
        }
    }

    /// A stdout that refuses every write, as a closed pipe does.
    struct Closed;

    impl io::Write for Closed {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from(io::ErrorKind::BrokenPipe))
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn args(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    /// Runs `answer` as an adapter answering `ping`: its exit code, stdout,
    /// stderr, and whether it read stdin.
    fn ran(
        words: &[&str],
        stdin: &[u8],
        answer: impl FnOnce(&str, &str) -> Result<String, Error>,
    ) -> (u8, String, String, bool) {
        let read = Cell::new(false);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_on(
            args(words),
            Watched(&read, stdin),
            &mut out,
            &mut err,
            &["ping"],
            answer,
        );
        (
            code,
            String::from_utf8_lossy(&out).into_owned(),
            String::from_utf8_lossy(&err).into_owned(),
            read.get(),
        )
    }

    #[test]
    fn a_known_verb_answers_one_line_on_stdout_and_exits_0() {
        let mut seen = None;
        let (code, out, err, _) = ran(&["ping"], b"{\"n\":1}", |verb, request| {
            seen = Some((verb.to_owned(), request.to_owned()));
            Ok("{\"pong\":1}".into())
        });
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (ANSWERED, "{\"pong\":1}\n", "")
        );
        assert_eq!(seen, Some(("ping".into(), "{\"n\":1}".into())));
    }

    #[test]
    fn an_unknown_or_missing_verb_exits_2_without_reading_stdin() {
        for words in [&["pong"][..], &[][..]] {
            let (code, out, err, read) = ran(words, b"{}", |_, _| Ok("{}".into()));
            assert_eq!(
                (code, out.as_str(), read),
                (UNKNOWN_VERB, "", false),
                "{words:?}"
            );
            assert!(err.contains("ping"), "{err}");
        }
    }

    #[test]
    fn a_verb_that_is_not_utf8_is_unknown() {
        use std::os::unix::ffi::OsStringExt;
        let read = Cell::new(false);
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_on(
            vec![OsString::from_vec(vec![0xff, b'p'])],
            Watched(&read, b""),
            &mut out,
            &mut err,
            &["ping"],
            |_, _| Ok("{}".into()),
        );
        assert_eq!((code, read.get()), (UNKNOWN_VERB, false));
    }

    #[test]
    fn a_failure_exits_1_with_why_on_stderr_and_nothing_on_stdout() {
        let (code, out, err, _) = ran(&["ping"], b"{}", |_, _| Err(Error::new("no pong here")));
        assert_eq!(
            (code, out.as_str(), err.as_str()),
            (FAILED, "", "no pong here\n")
        );
    }

    #[test]
    fn a_request_longer_than_max_json_fails_without_answering() {
        let long = vec![b' '; usize::try_from(MAX_JSON).unwrap_or(usize::MAX) + 1];
        let mut answered = false;
        let (code, out, err, _) = ran(&["ping"], &long, |_, _| {
            answered = true;
            Ok("{}".into())
        });
        assert_eq!((code, out.as_str(), answered), (FAILED, "", false));
        assert!(err.contains("longer than"), "{err}");
    }

    #[test]
    fn a_closed_stdout_exits_1() {
        let read = Cell::new(false);
        let mut err = Vec::new();
        let code = run_on(
            args(&["ping"]),
            Watched(&read, b"{}"),
            &mut Closed,
            &mut err,
            &["ping"],
            |_, _| Ok("{}".into()),
        );
        assert_eq!(code, FAILED);
        assert!(String::from_utf8_lossy(&err).contains("writing the answer"));
    }

    #[derive(serde::Deserialize)]
    struct Ask {
        n: u32,
    }

    #[test]
    fn handle_reads_the_request_and_writes_the_answer() {
        let doubled = handle(r#"{"n":2}"#, |ask: Ask| {
            Ok(serde_json::json!({ "n": ask.n * 2 }))
        });
        assert_eq!(doubled, Ok(r#"{"n":4}"#.to_owned()));
    }

    #[test]
    fn handle_refuses_a_request_of_the_wrong_shape_or_not_json() {
        for request in [r#"{"m":2}"#, "not json", ""] {
            let answer = handle(request, |ask: Ask| Ok(ask.n));
            assert!(
                answer
                    .as_ref()
                    .is_err_and(|e| e.to_string().starts_with("the request: ")),
                "{request:?}: {answer:?}"
            );
        }
    }

    #[test]
    fn find_program_skips_relative_empty_and_non_executable_entries() -> io::Result<()> {
        let root = TempDir::new("find")?;
        let (a, b, c) = (
            root.path().join("a"),
            root.path().join("b"),
            root.path().join("c"),
        );
        fs::create_dir_all(&a)?;
        fs::write(a.join("tool"), "#!/bin/sh\n")?;
        fs::set_permissions(a.join("tool"), fs::Permissions::from_mode(0o644))?;
        fs::create_dir_all(b.join("tool"))?;
        fs::create_dir_all(&c)?;
        fs::write(c.join("tool"), "#!/bin/sh\n")?;
        fs::set_permissions(c.join("tool"), fs::Permissions::from_mode(0o755))?;
        let path = OsString::from(format!(
            "rel::{}:{}:{}",
            a.display(),
            b.display(),
            c.display()
        ));
        assert_eq!(find_program("tool", Some(&path)), Some(c.join("tool")));
        assert_eq!(find_program("tool", Some(a.as_os_str())), None);
        assert_eq!(find_program("tool", None), None);
        Ok(())
    }

    #[test]
    fn config_dir_is_the_variable_or_under_home() {
        let env = |name: &str| match name {
            "TOOL_DIR" => Some(OsString::from("/cfg")),
            "HOME" => Some(OsString::from("/home/me")),
            _ => None,
        };
        assert_eq!(config_dir("TOOL_DIR", ".tool", env), Some("/cfg".into()));
        let env = |name: &str| match name {
            "TOOL_DIR" => Some(OsString::new()),
            "HOME" => Some(OsString::from("/home/me")),
            _ => None,
        };
        assert_eq!(
            config_dir("TOOL_DIR", ".tool", env),
            Some("/home/me/.tool".into())
        );
        let env = |name: &str| (name == "HOME").then(OsString::new);
        assert_eq!(config_dir("TOOL_DIR", ".tool", env), None);
        assert_eq!(config_dir("TOOL_DIR", ".tool", |_| None), None);
    }

    #[test]
    fn config_dir_ignores_a_relative_variable_or_home() {
        let env = |name: &str| match name {
            "TOOL_DIR" => Some(OsString::from("rel/cfg")),
            "HOME" => Some(OsString::from("/home/me")),
            _ => None,
        };
        assert_eq!(
            config_dir("TOOL_DIR", ".tool", env),
            Some("/home/me/.tool".into())
        );
        let env = |name: &str| (name == "HOME").then(|| OsString::from("home/me"));
        assert_eq!(config_dir("TOOL_DIR", ".tool", env), None);
    }

    #[test]
    fn read_json_tells_missing_broken_and_read_apart() -> io::Result<()> {
        let dir = TempDir::new("json")?;
        let file = dir.path().join("f.json");
        let read = || read_json::<serde_json::Value>(&file);
        assert_eq!(read(), JsonFile::Missing);
        fs::write(&file, r#"{"a":1}"#)?;
        assert_eq!(read(), JsonFile::Read(serde_json::json!({"a": 1})));
        fs::write(&file, "{nope")?;
        assert_eq!(read(), JsonFile::Broken);
        let mut long = vec![b' '; usize::try_from(MAX_JSON).unwrap_or(usize::MAX) + 1];
        long.extend_from_slice(b"{}");
        fs::write(&file, long)?;
        assert_eq!(read(), JsonFile::Broken);
        fs::remove_file(&file)?;
        fs::create_dir(&file)?;
        assert_eq!(read(), JsonFile::Broken);
        Ok(())
    }
}
