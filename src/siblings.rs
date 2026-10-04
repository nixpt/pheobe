//! In-repo worktree placement (PHEOBE-51, squadron SQ-204).
//!
//! A repo that carries `.jagent/` keeps agent worktrees INSIDE it, at
//! `<repo>/.jagent/worktrees/<name>`, so an engine pointed at the repo never
//! needs permissions outside it. Relative sibling path-deps (`../other-repo`)
//! then resolve through `.jagent/worktrees/<sib> -> ../../../<sib>` links.
//! This is a dependency-free port of squadron's `worktree_scan_siblings` /
//! `worktree_link_siblings` (lib/worktree-path.sh), the same rule buckets
//! (BUCKETS-17) applies.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Where agent worktrees live inside a fleet repo.
pub const JAGENT_WORKTREES: &str = ".jagent/worktrees";

/// `<repo>/.jagent/worktrees/<branch, '/'→'-'>` when `repo` uses the fleet
/// layout (has a `.jagent/` dir); `None` otherwise.
pub fn in_repo_dest(repo: &Path, branch: &str) -> Option<PathBuf> {
    repo.join(".jagent")
        .is_dir()
        .then(|| repo.join(JAGENT_WORKTREES).join(branch.replace('/', "-")))
}

/// `path = "../…"` values on one manifest line.
fn path_deps(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(at) = rest.find("path") {
        rest = &rest[at + 4..];
        let after = rest.trim_start();
        let Some(after) = after.strip_prefix('=') else {
            continue;
        };
        let Some(quoted) = after.trim_start().strip_prefix('"') else {
            continue;
        };
        if let Some(end) = quoted.find('"') {
            let value = &quoted[..end];
            if value.starts_with("../") {
                out.push(value.to_string());
            }
            rest = &quoted[end + 1..];
        }
    }
    out
}

/// Normalize `dir/target` lexically; `Some(first)` when the result climbs
/// out of the repo by exactly one level (`../first/...`).
fn one_level_sibling(dir: &str, target: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    let mut ups = 0usize;
    for comp in dir.split('/').chain(target.split('/')) {
        match comp {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    ups += 1;
                }
            }
            other => parts.push(other),
        }
    }
    (ups == 1)
        .then(|| parts.first().map(|s| s.to_string()))
        .flatten()
}

fn tracked(repo: &Path, spec: &[&str]) -> Vec<(String, String)> {
    let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["ls-files", "-z", "-s", "--"])
        .args(spec)
        .output()
    else {
        return Vec::new();
    };
    out.stdout
        .split(|b| *b == 0)
        .filter_map(|ent| {
            let ent = String::from_utf8_lossy(ent);
            let (meta, path) = ent.split_once('\t')?;
            Some((
                meta.split_whitespace().next()?.to_string(),
                path.to_string(),
            ))
        })
        .collect()
}

fn parent_dir(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|d| d.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// Siblings `repo` reaches by climbing exactly one level out of itself, via
/// tracked Cargo.toml `path = "../…"` deps or tracked `../…` symlinks.
pub fn scan(repo: &Path) -> Vec<String> {
    let mut sibs: Vec<String> = Vec::new();
    let mut add = |dir: &str, target: &str| {
        if let Some(s) = one_level_sibling(dir, target) {
            if !sibs.contains(&s) {
                sibs.push(s);
            }
        }
    };
    for (_, path) in tracked(repo, &["Cargo.toml", "*/Cargo.toml"]) {
        let Ok(text) = std::fs::read_to_string(repo.join(&path)) else {
            continue;
        };
        for dep in text.lines().flat_map(path_deps) {
            add(&parent_dir(&path), &dep);
        }
    }
    for (mode, path) in tracked(repo, &["."]) {
        if mode != "120000" {
            continue;
        }
        if let Ok(target) = std::fs::read_link(repo.join(&path)) {
            let target = target.to_string_lossy().to_string();
            if target.starts_with("../") {
                add(&parent_dir(&path), &target);
            }
        }
    }
    sibs.sort();
    sibs
}

/// Create `<repo>/.jagent/worktrees/<sib> -> ../../../<sib>` for every
/// sibling that exists next to `repo`. Best-effort: problems are logged.
pub fn link(repo: &Path) {
    let dir = repo.join(JAGENT_WORKTREES);
    for sib in scan(repo) {
        let Some(outside) = repo.parent().map(|p| p.join(&sib)) else {
            continue;
        };
        if !outside.exists() {
            eprintln!(
                "pheobe: sibling '{sib}' not found at {} — not linked",
                outside.display()
            );
            continue;
        }
        let link = dir.join(&sib);
        match std::fs::symlink_metadata(&link) {
            Ok(m) if !m.file_type().is_symlink() => {
                eprintln!(
                    "pheobe: {} exists and is not a symlink — sibling '{sib}' not linked",
                    link.display()
                );
                continue;
            }
            Ok(_) => {
                let _ = std::fs::remove_file(&link);
            }
            Err(_) => {}
        }
        let _ = std::fs::create_dir_all(&dir);
        #[cfg(unix)]
        if let Err(e) = std::os::unix::fs::symlink(format!("../../../{sib}"), &link) {
            eprintln!("pheobe: could not link sibling '{sib}': {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_deps_reads_every_relative_path_on_a_line() {
        assert_eq!(
            path_deps(r#"a = { path = "../alpha" }, b = { version = "1", path="../../x/y" }"#),
            ["../alpha", "../../x/y"]
        );
        assert!(
            path_deps(r#"c = { path = "crates/c" }"#).is_empty(),
            "in-repo paths are not siblings"
        );
    }

    #[test]
    fn one_level_climbs_only() {
        assert_eq!(one_level_sibling("", "../alpha"), Some("alpha".into()));
        // crates/ai/Cargo.toml: ../../../uno is ONE level out of the repo
        assert_eq!(
            one_level_sibling("crates/ai", "../../../uno"),
            Some("uno".into())
        );
        assert_eq!(
            one_level_sibling("crates/ai", "../../../../far"),
            None,
            "two levels"
        );
        assert_eq!(one_level_sibling("crates/ai", "../b"), None, "stays inside");
    }

    #[test]
    fn in_repo_dest_only_for_fleet_repos() {
        let d = std::env::temp_dir().join(format!("pheobe-sib-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(in_repo_dest(&d, "pheobe/x"), None);
        std::fs::create_dir_all(d.join(".jagent")).unwrap();
        assert_eq!(
            in_repo_dest(&d, "pheobe/x"),
            Some(d.join(".jagent/worktrees/pheobe-x"))
        );
        std::fs::remove_dir_all(&d).ok();
    }
}
