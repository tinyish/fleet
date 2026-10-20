//! The visible evals: every transcript case under `tests/cmd/`, run against
//! the built binary through `tests/cmd/bin/fleet`, which makes each case
//! hermetic (CONTRIBUTING § Evals, `tests/cmd/README.md`).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// The commands a case may run; anything else would be skipped by trycmd,
/// not failed, so a case could pass without its key step.
const COMMANDS: [&str; 9] = [
    "fleet", "in", "repo", "write", "gone", "units", "git", "cat", "ls",
];

/// Kills every supervisor the fakes started that still runs once all cases
/// are done, so a case that failed part way never leaves one behind. Only a
/// process that is still an eval's supervisor is killed.
struct Reaper(PathBuf);

impl Drop for Reaper {
    fn drop(&mut self) {
        let pids = fs::read_to_string(&self.0).unwrap_or_default();
        for pid in pids.split_whitespace() {
            let command = Command::new("ps")
                .args(["-o", "command=", "-p", pid])
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
                .unwrap_or_default();
            if command.contains("/.fleet/bin/fleet supervise") {
                let _ = Command::new("kill").arg(pid).stderr(Stdio::null()).status();
            }
        }
        let _ = fs::remove_file(&self.0);
    }
}

fn cases_with(cases: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for verb in fs::read_dir(cases).into_iter().flatten().flatten() {
        for case in fs::read_dir(verb.path()).into_iter().flatten().flatten() {
            let path = case.path();
            if path.extension().is_some_and(|e| e == extension) {
                found.push(path);
            }
        }
    }
    found
}

/// Every case runs in a temp directory of its own (a `.md` has a `.out/`, a
/// `.toml` sets `fs.sandbox`), and runs only commands the harness knows.
fn check_cases(cases: &Path) {
    for case in cases_with(cases, "toml") {
        let text = fs::read_to_string(&case).unwrap_or_default();
        assert!(
            text.contains("fs.sandbox = true"),
            "{} does not set fs.sandbox = true: it would run inside the repository",
            case.display()
        );
        assert!(
            text.contains("bin.name = \"fleet\""),
            "{}: a .toml case runs fleet",
            case.display()
        );
    }
    for case in cases_with(cases, "md") {
        assert!(
            case.with_extension("out").is_dir(),
            "{} has no {}: it would run inside the repository",
            case.display(),
            case.with_extension("out").display()
        );
        let text = fs::read_to_string(&case).unwrap_or_default();
        for line in text.lines().filter_map(|l| l.strip_prefix("$ ")) {
            let command = line.split_whitespace().next().unwrap_or_default();
            assert!(
                COMMANDS.contains(&command),
                "{}: `{command}` is not a command the evals provide",
                case.display()
            );
        }
    }
}

/// The evals reach the service manager only because its driver runs
/// `launchctl` and `systemctl` by name, which the fleet shim's PATH resolves
/// to the fakes. A driver naming a tool by its path would reach the real
/// one, so no eval runs while one does.
fn check_tools_run_by_name(repo: &Path) {
    let service = repo.join("crates/service/src");
    for file in fs::read_dir(&service).into_iter().flatten().flatten() {
        let text = fs::read_to_string(file.path()).unwrap_or_default();
        for call in ["run(\"", "Command::new(\""] {
            for (at, _) in text.match_indices(call) {
                let tool = text[at + call.len()..]
                    .split('"')
                    .next()
                    .unwrap_or_default();
                assert!(
                    !tool.contains('/'),
                    "{}: `{tool}` is run by its path; the evals need it by name",
                    file.path().display()
                );
            }
        }
    }
}

#[test]
fn evals() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let repo = fs::canonicalize(&repo).unwrap_or(repo);
    let cases = repo.join("tests/cmd");
    let bin = cases.join("bin");
    check_cases(&cases);
    check_tools_run_by_name(&repo);
    let pids = std::env::temp_dir().join(format!("fleet-eval-pids-{}", std::process::id()));
    let _reaper = Reaper(pids.clone());
    trycmd::TestCases::new()
        .register_bin("fleet", bin.join("fleet"))
        .register_bin("in", bin.join("in"))
        .register_bin("repo", bin.join("repo"))
        .register_bin("write", bin.join("write"))
        .register_bin("gone", bin.join("gone"))
        .register_bin("units", bin.join("units"))
        .register_bin("git", bin.join("git"))
        .register_bin("cat", PathBuf::from("/bin/cat"))
        .register_bin("ls", PathBuf::from("/bin/ls"))
        .env("FLEET_EVAL_BIN", env!("CARGO_BIN_EXE_fleet"))
        .env("FLEET_EVAL_REPO", repo.display().to_string())
        .env("FLEET_EVAL_PIDS", pids.display().to_string())
        .env("FLEET_EVAL_ROOT", "")
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("LC_ALL", "C")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .timeout(Duration::from_secs(30))
        .case(cases.join("*/*.md"))
        .case(cases.join("*/*.toml"));
}
