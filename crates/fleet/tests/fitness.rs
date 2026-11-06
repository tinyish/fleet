//! The fitness tests: the rules `brain/architecture/core.md` sets on the
//! workspace (§ Crates and dependency rules, § Invariants, § Toolchain) and
//! the size budgets of `brain/architecture/conventions.md#size-budgets`,
//! read from the workspace's dependency graph (`cargo metadata`) and its
//! source. A failure names the rule the code breaks; the rule is never
//! loosened to make it pass (AGENTS.md).
//!
//! Dev-dependencies are left out of the dependency rules: they never reach
//! the binary, so a test may use what the code it tests may not.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

const DRIVERS: [&str; 5] = ["service", "git", "tmux", "adapter-host", "curl"];
const CRATES: &str = "brain/architecture/core.md § Crates and dependency rules";
const SIZES: &str = "brain/architecture/conventions.md#size-budgets";

/// A crate of the workspace, as `cargo metadata` reads its manifest.
struct Crate {
    name: String,
    dir: PathBuf,
    /// Every dependency: (package name, the name its source calls it by,
    /// whether it is a dev-dependency).
    deps: Vec<(String, String, bool)>,
}

fn workspace() -> Vec<Crate> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--no-deps", "--offline", "--format-version=1"])
        .arg("--manifest-path")
        .arg(manifest)
        .output();
    let json: Value = output
        .ok()
        .and_then(|o| serde_json::from_slice(&o.stdout).ok())
        .unwrap_or_default();
    let crates: Vec<Crate> = json["packages"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| Crate {
            name: p["name"].as_str().unwrap_or_default().to_owned(),
            dir: Path::new(p["manifest_path"].as_str().unwrap_or_default())
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default(),
            deps: p["dependencies"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|d| {
                    let name = d["name"].as_str().unwrap_or_default().to_owned();
                    let called = d["rename"]
                        .as_str()
                        .map_or_else(|| name.replace('-', "_"), str::to_owned);
                    (name, called, d["kind"] == "dev")
                })
                .collect(),
        })
        .collect();
    assert!(
        crates.iter().any(|c| c.name == "fleet"),
        "cargo metadata read no workspace"
    );
    crates
}

/// The crates of the workspace `c` depends on, dev-dependencies left out.
fn workspace_deps<'a>(c: &'a Crate, all: &'a [Crate]) -> impl Iterator<Item = &'a str> {
    c.deps
        .iter()
        .filter(|(name, _, dev)| !dev && all.iter().any(|w| &w.name == name))
        .map(|(name, _, _)| name.as_str())
}

#[test]
fn only_the_supervisor_depends_on_state() {
    let all = workspace();
    for c in all.iter().filter(|c| c.name != "supervisor") {
        assert!(
            !workspace_deps(c, &all).any(|d| d == "state"),
            "`{}` depends on `state`, which only `supervisor` may, so the CLI cannot write state ({CRATES})",
            c.name
        );
    }
}

#[test]
fn core_depends_only_on_the_sdk_and_the_sdk_on_nothing() {
    let all = workspace();
    for (name, allowed) in [
        ("fleet-core", &["fleet-adapter-sdk"][..]),
        ("fleet-adapter-sdk", &[]),
    ] {
        let c = all.iter().find(|c| c.name == name);
        for dep in c.into_iter().flat_map(|c| workspace_deps(c, &all)) {
            assert!(
                allowed.contains(&dep),
                "`{name}` depends on `{dep}`: `core` may depend only on `fleet-adapter-sdk` in the workspace, and `fleet-adapter-sdk` on nothing in it ({CRATES})"
            );
        }
    }
}

#[test]
fn drivers_depend_only_on_core() {
    let all = workspace();
    for c in all.iter().filter(|c| DRIVERS.contains(&c.name.as_str())) {
        for dep in workspace_deps(c, &all) {
            assert!(
                dep == "fleet-core",
                "the driver `{}` depends on `{dep}`: a driver depends on nothing in the workspace but `core` ({CRATES})",
                c.name
            );
        }
    }
}

/// Invariant 7, and the last of the dependency rules: no crate but a driver
/// names `std::process::Command`, however it is imported. The SDK's `kit` is
/// left out, as core.md says: it is not part of tinyfleet.
#[test]
fn only_drivers_start_programs() {
    for c in workspace() {
        if DRIVERS.contains(&c.name.as_str()) {
            continue;
        }
        let kit: &[&str] = if c.name == "fleet-adapter-sdk" {
            &["feature = \"kit\""]
        } else {
            &[]
        };
        for (file, code) in code(&c.dir, kit) {
            assert!(
                !starts_program(&code),
                "{} starts an outside program: only a driver may ({CRATES}; core.md § Invariants, 7)",
                file.display()
            );
        }
    }
}

/// `conventions.md#size-budgets`: functions are held by clippy's
/// `too_many_lines` (`clippy.toml`), files here.
#[test]
fn files_keep_their_size_budget() {
    for c in workspace() {
        for (file, code) in code(&c.dir, &[]) {
            let lines = code.lines().count();
            assert!(
                lines <= 600,
                "{} has {lines} lines besides its inline tests; a file has at most 600 ({SIZES})",
                file.display()
            );
        }
    }
}

/// `conventions.md#size-budgets`: a crate's source names every crate it
/// depends on, as `name::`.
#[test]
fn every_dependency_is_used() {
    for c in workspace() {
        let source: String = rs_files(&c.dir)
            .iter()
            .map(|f| without_comments(&fs::read_to_string(f).unwrap_or_default()))
            .collect::<Vec<_>>()
            .join("\n");
        for (name, called, _) in &c.deps {
            let path = format!("{called}::");
            let named = source.match_indices(&path).any(|(at, _)| {
                source[..at]
                    .chars()
                    .next_back()
                    .is_none_or(|b| !(b.is_alphanumeric() || b == '_' || b == ':'))
            });
            assert!(
                named,
                "`{}` depends on `{name}` but its source never names `{path}`: no unused dependencies ({SIZES})",
                c.name
            );
        }
    }
}

/// core.md § Toolchain: clippy runs with a pinned lint set, the workspace's,
/// which a crate keeps only by inheriting it.
#[test]
fn every_crate_takes_the_workspace_lints() {
    for c in workspace() {
        let manifest = fs::read_to_string(c.dir.join("Cargo.toml")).unwrap_or_default();
        assert!(
            manifest.contains("\n[lints]\nworkspace = true\n"),
            "`{}`'s Cargo.toml has no `[lints] workspace = true`: every crate takes the workspace's pinned lint set (brain/architecture/core.md § Toolchain)",
            c.name
        );
    }
}

/// Whether `code` names `std::process::Command`: by its path, in a group
/// imported from `process`, or by a glob of `process`.
fn starts_program(code: &str) -> bool {
    let code: String = without_comments(code).split_whitespace().collect();
    code.contains("process::Command")
        || code.contains("process::*")
        || code.match_indices("process::{").any(|(at, _)| {
            code[at..]
                .split('}')
                .next()
                .is_some_and(|group| group.split([',', '{']).any(|w| w.starts_with("Command")))
        })
}

fn without_comments(code: &str) -> String {
    code.lines()
        .map(|l| l.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The code of each file under `dir/src` that is not a test: inline
/// `#[cfg(test)] mod … { }` blocks are cut, and files of a module declared
/// `#[cfg(test)] mod name;`, or gated by one of `gates`, are left out.
fn code(dir: &Path, gates: &[&str]) -> Vec<(PathBuf, String)> {
    let files = rs_files(&dir.join("src"));
    let mut left_out = Vec::new();
    for file in &files {
        let text = fs::read_to_string(file).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        for pair in lines.windows(2) {
            let gate = pair[0]
                .strip_prefix("#[cfg(")
                .and_then(|g| g.strip_suffix(")]"));
            let module = pair[1]
                .strip_suffix(';')
                .filter(|d| d.split_whitespace().rev().nth(1) == Some("mod"))
                .and_then(|d| d.split_whitespace().last());
            if let (Some(gate), Some(module)) = (gate, module)
                && (gate == "test" || gates.contains(&gate))
            {
                let stem = file.file_stem().unwrap_or_default();
                let home = if ["lib", "main", "mod"].iter().any(|s| stem == *s) {
                    file.parent().map(Path::to_path_buf).unwrap_or_default()
                } else {
                    file.with_extension("")
                };
                left_out.push(home.join(format!("{module}.rs")));
                left_out.push(home.join(module));
            }
        }
    }
    files
        .into_iter()
        .filter(|f| !left_out.iter().any(|g| f.starts_with(g)))
        .map(|f| {
            let code = without_tests(&fs::read_to_string(&f).unwrap_or_default());
            (f, code)
        })
        .collect()
}

/// `text` without its top-level `#[cfg(test)] mod … { }` blocks, which
/// rustfmt closes with a `}` at the start of a line.
fn without_tests(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut kept = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let opens_tests = lines[i] == "#[cfg(test)]"
            && lines.get(i + 1).is_some_and(|l| {
                !l.starts_with(' ') && l.ends_with(" {") && l.split_whitespace().any(|w| w == "mod")
            });
        if opens_tests {
            while i < lines.len() && lines[i] != "}" {
                i += 1;
            }
        } else {
            kept.push(lines[i]);
        }
        i += 1;
    }
    kept.join("\n")
}

fn rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() && path.file_name().is_some_and(|n| n != "target") {
            found.extend(rs_files(&path));
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(path);
        }
    }
    found
}
