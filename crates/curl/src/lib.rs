//! The network's driver: downloads with the person's `curl`. A driver, so
//! it may start programs; it depends on nothing in the workspace but
//! `fleet-core`.

use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use fleet_core::tools::{Fetched, Network, ToolError};

/// The most of one download, in bytes: far more than any adapter.
pub const MAX_BYTES: u64 = 64 << 20;

/// `curl`, run with time and size limits, over HTTPS only unless told
/// otherwise.
#[derive(Debug, Clone)]
pub struct Curl {
    program: OsString,
    files: bool,
}

impl Curl {
    /// The `curl` on the `PATH`; `files` also allows `file://` URLs, for a
    /// release served from a directory (`FLEET_ADAPTERS_URL`).
    #[must_use]
    pub fn new(files: bool) -> Curl {
        Curl {
            program: "curl".into(),
            files,
        }
    }

    #[cfg(test)]
    fn with_program(program: &str, files: bool) -> Curl {
        Curl {
            program: program.into(),
            files,
        }
    }
}

impl Network for Curl {
    fn download(&self, url: &str, to: &Path) -> Result<Fetched, ToolError> {
        let proto = if self.files { "=https,file" } else { "=https" };
        // -q, first: never read the person's ~/.curlrc, which could turn
        // off the TLS checks the SHA-256 rests on.
        let out = Command::new(&self.program)
            .args(["-q", "-fsSL", "--connect-timeout", "10", "--max-time", "60"])
            .args(["--max-filesize", &MAX_BYTES.to_string()])
            .args(["--proto", proto, "--proto-redir", "=https", "-o"])
            .arg(to)
            .arg(url)
            .output()
            .map_err(|error| ToolError::CouldNotRun {
                program: "curl".into(),
                error,
            })?;
        let said = String::from_utf8_lossy(&out.stderr).trim().to_owned();
        match out.status.code() {
            Some(0) => Ok(Fetched::Saved),
            code if nothing_there(code, &said) => Ok(Fetched::NotFound),
            _ => Err(ToolError::Failed {
                command: format!("curl {url}"),
                said,
            }),
        }
    }

    fn version(&self) -> Result<String, ToolError> {
        let out = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|error| ToolError::CouldNotRun {
                program: "curl".into(),
                error,
            })?;
        if !out.status.success() {
            return Err(ToolError::Failed {
                command: "curl --version".into(),
                said: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            });
        }
        Ok(short_version(&String::from_utf8_lossy(&out.stdout)))
    }
}

/// `curl --version`'s name and number, its first line up to the build
/// details in parentheses: `curl 8.7.1`.
fn short_version(said: &str) -> String {
    let first = said.lines().next().unwrap_or_default();
    first.split(" (").next().unwrap_or(first).trim().to_owned()
}

/// Whether curl, exiting with `code` and saying `said`, found nothing at
/// the URL: an HTTP 404 (exit 22 under `-f`, or 56 over HTTP/2), or a
/// `file://` path that is not there (37).
fn nothing_there(code: Option<i32>, said: &str) -> bool {
    match code {
        Some(22 | 56) => said.contains("returned error: 404"),
        Some(37) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    use fleet_core::tools::{Fetched, Network, ToolError};

    /// A temp directory of the test's own, deleted at the end.
    struct Dir(PathBuf);

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn dir(name: &str) -> Dir {
        let d = std::env::temp_dir().join(format!("fleet-curl-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let _ = fs::create_dir_all(&d);
        Dir(d)
    }

    #[test]
    fn a_file_url_downloads_byte_for_byte() {
        let d = dir("saved");
        let from = d.0.join("from");
        let bytes: Vec<u8> = (0..=255).collect();
        let _ = fs::write(&from, &bytes);
        let to = d.0.join("to");
        let url = format!("file://{}", from.display());
        assert_eq!(
            Curl::new(true).download(&url, &to).ok(),
            Some(Fetched::Saved)
        );
        assert_eq!(fs::read(&to).ok(), Some(bytes));
    }

    #[test]
    fn a_missing_file_is_not_found() {
        let d = dir("missing");
        let url = format!("file://{}", d.0.join("nothing").display());
        assert_eq!(
            Curl::new(true).download(&url, &d.0.join("to")).ok(),
            Some(Fetched::NotFound)
        );
    }

    #[test]
    fn file_urls_are_refused_unless_allowed() {
        let d = dir("refused");
        let from = d.0.join("from");
        let _ = fs::write(&from, "x");
        let url = format!("file://{}", from.display());
        assert!(matches!(
            Curl::new(false).download(&url, &d.0.join("to")),
            Err(ToolError::Failed { .. })
        ));
    }

    #[test]
    fn no_curl_could_not_run() {
        let d = dir("nocurl");
        assert!(matches!(
            Curl::with_program("/nonexistent/curl", true).download("file:///x", &d.0.join("to")),
            Err(ToolError::CouldNotRun { .. })
        ));
    }

    #[test]
    fn curl_never_reads_the_persons_curlrc() {
        // A config there could turn off TLS checks the SHA-256 rests on;
        // -q must come first to be heeded.
        let d = dir("curlrc");
        let fake = d.0.join("curl");
        let args = d.0.join("args");
        let _ = fs::write(
            &fake,
            format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n", args.display()),
        );
        let _ = fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755));
        let _ = Curl::with_program(&fake.display().to_string(), false)
            .download("https://x", &d.0.join("to"));
        let said = fs::read_to_string(&args).unwrap_or_default();
        assert_eq!(said.lines().next(), Some("-q"), "{said}");
    }

    #[test]
    fn a_404_is_nothing_there_whatever_exit_curl_gives_it() {
        // curl reports an HTTP error under -f as 22, or as 56 over HTTP/2.
        let said = "curl: (56) The requested URL returned error: 404";
        assert!(nothing_there(Some(56), said));
        assert!(nothing_there(Some(22), &said.replace("(56)", "(22)")));
        assert!(nothing_there(Some(37), "curl: (37) Couldn't open file /x"));
        assert!(!nothing_there(
            Some(22),
            "curl: (22) The requested URL returned error: 500"
        ));
        assert!(!nothing_there(
            Some(6),
            "curl: (6) Could not resolve host: github.com"
        ));
    }

    #[test]
    fn version_is_curls_name_and_number() {
        let version = Curl::new(false).version().unwrap_or_default();
        assert!(version.starts_with("curl "), "{version}");
        assert!(!version.contains(['\n', '(']), "{version}");
        assert_eq!(
            short_version(
                "curl 8.7.1 (x86_64-apple-darwin25.0) libcurl/8.7.1 zlib/1.2.12\nRelease-Date: x"
            ),
            "curl 8.7.1"
        );
    }
}
