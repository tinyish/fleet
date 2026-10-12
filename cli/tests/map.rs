//! The architecture map against the tree. Every Rust file in the three crates
//! is owned by exactly one area's `brain/specs/<area>/mod.md`, and every path
//! an area owns is a file. CONTRIBUTING.md § Specs is the rule pinned here.
//!
//! Ownership is narrow on purpose: only a `- `-line under an area's
//! `## Modules` heading owns a path, the first backticked one on that line.
//! A path named in a description, a flow or the overview owns nothing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the cli crate sits inside the workspace")
        .to_path_buf()
}

/// Every Rust file the map must place: each crate's `src/` tree, its
/// `examples/` and its build script. `tests/` is not mapped.
fn modules(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for krate in ["core", "controller", "cli"] {
        for dir in ["src", "examples"] {
            walk(root, &root.join(krate).join(dir), &mut found);
        }
        let build = root.join(krate).join("build.rs");
        if build.is_file() {
            found.push(relative(root, &build));
        }
    }
    found.sort();
    found
}

fn walk(root: &Path, dir: &Path, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            walk(root, &path, found);
        } else if path.extension().is_some_and(|e| e == "rs") {
            found.push(relative(root, &path));
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("the path is under the workspace")
        .to_string_lossy()
        .replace('\\', "/")
}

/// Each owned path and the areas that own it, once per ownership line.
fn owners(root: &Path) -> BTreeMap<String, Vec<String>> {
    let mut owned: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut areas: Vec<PathBuf> = std::fs::read_dir(root.join("brain/specs"))
        .expect("brain/specs exists")
        .map(|e| e.expect("a directory entry").path())
        .filter(|p| p.join("mod.md").is_file())
        .collect();
    areas.sort();
    for area in areas {
        let name = area
            .file_name()
            .expect("an area folder has a name")
            .to_string_lossy()
            .into_owned();
        let text = std::fs::read_to_string(area.join("mod.md")).expect("mod.md reads");
        let mut in_modules = false;
        for line in text.lines() {
            if line.starts_with("## ") {
                in_modules = line.trim_end() == "## Modules";
                continue;
            }
            if let (true, Some(rest)) = (in_modules, line.strip_prefix("- `")) {
                let path = rest.split('`').next().unwrap_or_default().to_string();
                owned.entry(path).or_default().push(name.clone());
            }
        }
    }
    owned
}

#[test]
fn every_module_is_named_by_exactly_one_area() {
    let root = root();
    let owned = owners(&root);
    let drift: Vec<String> = modules(&root)
        .into_iter()
        .filter_map(|m| match owned.get(&m) {
            None => Some(format!("{m}: no area's mod.md names it")),
            Some(areas) if areas.len() > 1 => Some(format!(
                "{m}: named {} times ({})",
                areas.len(),
                areas.join(", ")
            )),
            Some(_) => None,
        })
        .collect();
    assert!(drift.is_empty(), "map drift:\n{}", drift.join("\n"));
}

#[test]
fn every_name_in_the_map_is_a_module() {
    let root = root();
    let all = modules(&root);
    let stale: Vec<String> = owners(&root)
        .into_keys()
        .filter(|p| !all.contains(p))
        .collect();
    assert!(
        stale.is_empty(),
        "the map names files that are not modules:\n{}",
        stale.join("\n")
    );
}
