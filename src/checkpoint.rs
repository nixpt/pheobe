//! Named working-state snapshots — jokersquad `checkpoint` discipline ported
//! to pheobe's shape (git stash create plumbing). `create` NEVER touches the
//! working tree: `git stash create` only mints commit objects and `stash
//! store` records the ref — work continues uninterrupted. The registry is
//! `.pheobe/checkpoints.json` (upsert by name). `restore` is the one
//! tree-touching verb (stash apply). `prune` keeps the newest N (default 5),
//! dropping their stash refs best-effort.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    pub name: String,
    /// Unix-epoch seconds (no chrono dep — ordering is all the registry needs)
    pub ts: String,
    pub head: String,
    /// `git stash create` sha; None = tree was clean at checkpoint time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stash: Option<String>,
}

fn registry(worktree: &Path) -> std::path::PathBuf {
    worktree.join(".pheobe").join("checkpoints.json")
}

fn git(wt: &Path, args: &[&str]) -> Result<String> {
    let out = std::process::Command::new("git").arg("-C").arg(wt).args(args).output()?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    if !out.status.success() {
        bail!("git {} failed: {}", args.join(" "), combined.trim());
    }
    Ok(combined.trim().to_string())
}

fn load(worktree: &Path) -> Result<Vec<Checkpoint>> {
    let p = registry(worktree);
    if !p.exists() {
        return Ok(vec![]);
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(p)?)?)
}

fn save(worktree: &Path, cps: &[Checkpoint]) -> Result<()> {
    std::fs::create_dir_all(worktree.join(".pheobe"))?;
    std::fs::write(registry(worktree), serde_json::to_string_pretty(cps)?)?;
    Ok(())
}

fn short(sha: &str) -> String {
    sha.chars().take(8).collect()
}

/// Snapshot the current dirty state under `name`. Never touches the working tree.
pub fn create(worktree: &Path, name: &str) -> Result<String> {
    if name.is_empty()
        || !name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
    {
        bail!("checkpoint name must be letters, digits, dot, dash, or underscore: '{name}'");
    }
    let head = git(worktree, &["rev-parse", "HEAD"])?;
    let stash_sha = git(worktree, &["stash", "create"])?;
    let stash = if stash_sha.is_empty() {
        None
    } else {
        git(worktree, &["stash", "store", "-m", &format!("pheobe-checkpoint: {name}"), &stash_sha])?;
        Some(stash_sha)
    };
    let mut cps = load(worktree)?;
    cps.retain(|c| c.name != name); // upsert by name
    let msg = match &stash {
        Some(s) => format!("created '{}' (head={}, stash={})", name, short(&head), short(s)),
        None => format!("created '{}' (head={}, tree was clean)", name, short(&head)),
    };
    cps.push(Checkpoint { name: name.into(), ts: now_ts(), head, stash });
    save(worktree, &cps)?;
    Ok(msg)
}

pub fn list(worktree: &Path) -> Result<Vec<Checkpoint>> {
    load(worktree)
}

/// Restore a named snapshot onto the current tree (the one tree-touching verb).
pub fn restore(worktree: &Path, name: &str) -> Result<String> {
    let cp = match list(worktree)?.into_iter().rev().find(|c| c.name == name) {
        Some(c) => c,
        None => bail!("no checkpoint named '{name}'"),
    };
    let stash = match &cp.stash {
        Some(s) => s.clone(),
        None => bail!("checkpoint '{name}' had no uncommitted state to restore (tree was clean)"),
    };
    git(worktree, &["stash", "apply", &stash])?;
    Ok(format!("restored '{name}' — applied stash {}", short(&stash)))
}

/// Keep only the newest `keep` checkpoints; pruned stash objects are dropped
/// from the stash list best-effort.
pub fn prune(worktree: &Path, keep: usize) -> Result<String> {
    let mut cps = load(worktree)?;
    if cps.len() <= keep {
        return Ok(format!("{} <= keep={keep}, nothing to prune", cps.len()));
    }
    let split_at = cps.len() - keep;
    let dropped: Vec<Checkpoint> = cps.drain(..split_at).collect(); // newest stay in cps
    for c in &dropped {
        if let Some(s) = &c.stash {
            if let Ok(list) = git(worktree, &["stash", "list", "--format=%gd %H"]) {
                for line in list.lines() {
                    let mut it = line.split_whitespace();
                    if let (Some(r), Some(h)) = (it.next(), it.next()) {
                        if h == s {
                            let _ = git(worktree, &["stash", "drop", r]);
                        }
                    }
                }
            }
        }
    }
    save(worktree, &cps)?;
    Ok(format!("pruned {}, kept {}", dropped.len(), keep))
}

fn now_ts() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default()
}
