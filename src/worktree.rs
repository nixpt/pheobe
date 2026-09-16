//! Worktree isolation — pheobe never cooks in the parent's checkout.
//! Ladder: kitchen (when present) > buckets worktree > plain git worktree add.
//! Warn when the given repo looks like a primary source checkout and
//! worktree was declined.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Kitchen {
    pub worktree: PathBuf,
    pub branch: String,
    pub repo: PathBuf,
}

fn run(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().context("spawn failed")?;
    if !out.status.success() {
        bail!(
            "command failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn is_source_checkout(repo: &Path) -> bool {
    repo.join(".git").is_dir()
}

/// Provision an isolated working copy. Returns (worktree path, branch).
pub fn provision(repo: &Path, branch: &str) -> Result<(PathBuf, String)> {
    if !is_source_checkout(repo) {
        // already a worktree — find the source repo and branch from it
        bail!("given repo is already a worktree; pass the source repo (kitchen resolves this)")
    }
    if kitchen_available() {
        return provision_kitchen(repo, branch);
    }
    if buckets_available() {
        let wt = run(Command::new("buckets")
            .args(["worktree", "create", &repo.display().to_string(), branch]))?;
        let wt = wt.lines().last().context("buckets printed nothing")?.trim().to_string();
        if !Path::new(&wt).is_dir() {
            bail!("buckets worktree create did not produce a path: {wt}");
        }
        return Ok((PathBuf::from(wt), branch.to_string()));
    }
    // plain git fallback
    let wt = repo.join(format!("../{}-{}", repo.file_name().and_then(|n| n.to_str()).unwrap_or("repo"), branch.replace('/', "-")));
    run(Command::new("git").arg("-C").arg(repo)
        .args(["worktree", "add", &wt.to_string_lossy(), "-b", branch]))?;
    Ok((wt, branch.to_string()))
}

fn kitchen_available() -> bool {
    which("kitchen")
}
fn buckets_available() -> bool {
    which("buckets")
}
fn which(bin: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {bin} >/dev/null 2>&1")]).status().is_ok_and(|s| s.success())
}

fn provision_kitchen(repo: &Path, branch: &str) -> Result<(PathBuf, String)> {
    // kitchen enter branches agent/<agent>/<ticket>; we want the branch name we
    // were given, so use buckets directly for the worktree but reuse kitchen's
    // target-dir discipline via env (kept simple in v0: buckets is the engine).
    provision_buckets(repo, branch)
}

fn provision_buckets(repo: &Path, branch: &str) -> Result<(PathBuf, String)> {
    let wt = run(Command::new("buckets")
        .args(["worktree", "create", &repo.display().to_string(), branch]))?;
    let last = wt.lines().last().context("buckets printed nothing")?.trim().to_string();
    if !Path::new(&last).is_dir() {
        bail!("buckets worktree create did not produce a directory: {last}")
    }
    Ok((PathBuf::from(last), branch.to_string()))
}

pub fn status_dirty(wt: &Path) -> Result<bool> {
    let status = run(Command::new("git").arg("-C").arg(wt).args(["status", "--porcelain"]))?;
    Ok(!status.trim().is_empty())
}

/// Pre-commit scope check: staged/unstaged paths must sit inside paths_allow.
pub fn check_allowlist(wt: &Path, paths_allow: &[String]) -> Result<Vec<String>> {
    let status = run(Command::new("git").arg("-C").arg(wt)
        .args(["status", "--porcelain"]))?;
    let mut violations = vec![];
    for line in status.lines() {
        let path = line.get(3..).unwrap_or("").trim();
        if path.is_empty() {
            continue;
        }
        let inside = paths_allow
            .iter()
            .any(|a| {
                let a = a.trim_end_matches('/');
                path == a || path.starts_with(a)
            });
        if !inside {
            violations.push(path.to_string());
        }
    }
    Ok(violations)
}

/// Commit with a provenance trailer (borrowed from commit-msg-agent-trailer).
pub fn commit(wt: &Path, task_id: &str, message: &str) -> Result<String> {
    run(Command::new("git").arg("-C").arg(wt).args(["add", "-A"]))?;
    let trailer = format!("Pheobe-Task: {task_id}");
    let hash = run(Command::new("git").arg("-C").arg(wt).args([
        "-c", "user.name=pheobe", "-c", "user.email=pheobe@local",
        "commit", "-m", message, "-m", &trailer, "--no-verify", "--quiet",
    ]))?;
    let _ = hash;
    let sha = run(Command::new("git").arg("-C").arg(wt).args(["rev-parse", "--short", "HEAD"]))?;
    Ok(sha)
}

/// Ship: push the branch (never a protected one). Merge is the parent's job.
pub fn push(wt: &Path, branch: &str) -> Result<()> {
    match branch {
        "main" | "master" | "dev" => bail!("refusing to ship protected branch"),
        _ => {}
    }
    run(Command::new("git").arg("-C").arg(wt).args(["push", "-u", "origin", branch]))?;
    Ok(())
}
