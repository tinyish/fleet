//! `fleet debug`: launch, list and stop agents' sessions by hand, to try a
//! piece before its real verbs exist. It prints the supervisor's reply as
//! the protocol's JSON line, so it has no words of its own: like the
//! specimen's labels, what it shows is facts and code identifiers. Compiled
//! only with the `debug` feature, so a release build has no such command.

use std::ffi::OsString;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand};
use fleet_core::paths::{Os, state_dir};
use fleet_core::protocol::{self, Reply, Request, request};

use crate::client;

/// How long `fleet debug` waits for an answer: longer than a first fetch
/// of an adapter (curl's 60 s, then the adapter's own time), so a slow
/// fetch's or adapter's fault reaches it.
const WAIT: Duration = Duration::from_secs(90);

#[derive(Debug, Parser)]
#[command(name = "fleet debug")]
struct Args {
    #[command(subcommand)]
    what: What,
}

#[derive(Debug, Subcommand)]
enum What {
    /// Launch an agent through an adapter program.
    Launch {
        /// The adapter: a program by its path, or `<name>@<version>`.
        #[arg(long)]
        adapter: String,
        /// Where the agent works; the current directory by default.
        #[arg(long)]
        directory: Option<PathBuf>,
        /// The model to run.
        #[arg(long)]
        model: Option<String>,
        /// The first thing the agent is told.
        #[arg(long)]
        prompt: Option<String>,
    },
    /// Every session, with its state.
    List,
    /// Stop a session.
    Stop {
        /// The session's id.
        session: String,
    },
}

/// Runs `fleet debug …`; `args` start at `debug`. Exits 0 with a response,
/// 1 with a refusal, 2 on a usage error, 3 when the supervisor cannot be
/// asked.
pub(crate) fn run(args: &[OsString]) -> ExitCode {
    let args = match Args::try_parse_from(args) {
        Ok(args) => args,
        Err(err) => {
            let _ = err.print();
            return ExitCode::from(2);
        }
    };
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(e) => return failed(&e),
    };
    let request = request_for(args.what, &cwd);
    let Some(os) = Os::this() else {
        return failed(&"unsupported OS");
    };
    let socket =
        match state_dir(os, |name| std::env::var_os(name)).map(|dir| protocol::socket_path(&dir)) {
            Ok(Ok(socket)) => socket,
            Ok(Err(e)) => return failed(&e),
            Err(e) => return failed(&e),
        };
    match client::ask(&socket, &request, WAIT) {
        Ok(reply) => {
            let _ = protocol::write_reply(&mut io::stdout(), &reply);
            match reply {
                Reply::Response(_) => ExitCode::SUCCESS,
                Reply::Status(_) => ExitCode::from(1),
            }
        }
        Err(e) => failed(&e),
    }
}

/// The request `what` makes, with paths made absolute from `cwd`.
fn request_for(what: What, cwd: &Path) -> Request {
    Request::Session(match what {
        What::Launch {
            adapter,
            directory,
            model,
            prompt,
        } => request::Session::Launch {
            adapter: if adapter.contains('/') {
                cwd.join(adapter).display().to_string()
            } else {
                adapter
            },
            directory: directory.map_or_else(|| cwd.to_path_buf(), |d| cwd.join(d)),
            model,
            prompt,
        },
        What::List => request::Session::List,
        What::Stop { session } => request::Session::Stop { session },
    })
}

fn failed(error: &dyn std::fmt::Debug) -> ExitCode {
    let _ = writeln!(io::stderr(), "{error:?}");
    ExitCode::from(3)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fleet_core::protocol::{Request, request};
    use std::path::{Path, PathBuf};

    fn parsed(args: &[&str]) -> Option<Request> {
        Args::try_parse_from(args)
            .ok()
            .map(|a| request_for(a.what, Path::new("/c/app")))
    }

    #[test]
    fn launch_makes_paths_absolute_from_where_it_runs() {
        assert_eq!(
            parsed(&[
                "debug",
                "launch",
                "--adapter",
                "packs/claude",
                "--model",
                "sonnet"
            ]),
            Some(Request::Session(request::Session::Launch {
                adapter: "/c/app/packs/claude".into(),
                directory: PathBuf::from("/c/app"),
                model: Some("sonnet".into()),
                prompt: None,
            }))
        );
        assert_eq!(
            parsed(&[
                "debug",
                "launch",
                "--adapter",
                "/a",
                "--directory",
                "sub",
                "--prompt",
                "hi"
            ]),
            Some(Request::Session(request::Session::Launch {
                adapter: "/a".into(),
                directory: PathBuf::from("/c/app/sub"),
                model: None,
                prompt: Some("hi".into()),
            }))
        );
    }

    #[test]
    fn a_named_adapter_is_sent_as_written() {
        assert_eq!(
            parsed(&["debug", "launch", "--adapter", "claude-code@2.1.285"]),
            Some(Request::Session(request::Session::Launch {
                adapter: "claude-code@2.1.285".into(),
                directory: PathBuf::from("/c/app"),
                model: None,
                prompt: None,
            }))
        );
    }

    #[test]
    fn list_and_stop_ask_for_what_they_name() {
        assert_eq!(
            parsed(&["debug", "list"]),
            Some(Request::Session(request::Session::List))
        );
        assert_eq!(
            parsed(&["debug", "stop", "s1"]),
            Some(Request::Session(request::Session::Stop {
                session: "s1".into()
            }))
        );
    }

    #[test]
    fn stop_without_an_id_is_a_usage_error() {
        assert_eq!(parsed(&["debug", "stop"]), None);
    }
}
