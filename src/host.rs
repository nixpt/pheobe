//! Host-mode supervisor: pheobe owns the kitchen; the adopter's model drives
//! the edits. `setup` provisions; `finish` is the mechanical exit gate.

use anyhow::{bail, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::report::TestEvidence;
use crate::task::Task;
use crate::{plan, run, sandbox, verify, worktree};

#[derive(Debug, Serialize)]
pub struct Setup {
    pub ok: bool,
    pub task: String,
    pub branch: String,
    pub worktree: String,
}

#[derive(Debug, Serialize)]
pub struct Finish {
    pub ok: bool,
    pub tests: TestEvidence,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub violations: Vec<String>,
}

/// Intake + provision + empty plan. JSON on stdout is the host's handle.
pub fn setup(task: &Task, branch: Option<&str>) -> Result<Setup> {
    task.validate()?;
    let sandbox_tier_name = task.effective_sandbox()?;
    let sandbox_tier = sandbox::Tier::from_name(&sandbox_tier_name)?;
    sandbox::ensure_at_intake(&sandbox_tier)?;
    let repo = task.resolve_repo()?;
    if !task.worktree && Path::new(&repo).join(".git").is_dir() {
        bail!(
            "refusing to cook in the primary source checkout — set worktree:true (default) \
             or point repo at a worktree (the source checkout is shared: mom's kitchen rule)"
        );
    }
    let branch = branch
        .map(|b| b.to_string())
        .or(task.branch.clone())
        .unwrap_or_else(|| run::default_branch(&task.task));
    let (wt, branch) = worktree::provision(&repo, &branch)?;
    if let Err(e) = plan::save(
        &wt,
        &plan::Plan {
            task: task.task.clone(),
            steps: vec![],
        },
    ) {
        let _ = worktree::teardown(&repo, &wt, &branch);
        return Err(e);
    }
    Ok(Setup {
        ok: true,
        task: task.task.clone(),
        branch,
        worktree: wt.display().to_string(),
    })
}

/// `done_when` + `paths_allow` in the host's worktree.
pub fn finish(task: &Task, wt: &Path) -> Result<Finish> {
    let tests = verify::run_done_when(task, wt)?;
    let (violations, _byproducts) = worktree::check_allowlist(wt, &task.paths_allow)?;
    let ok = tests.passed && violations.is_empty();
    Ok(Finish {
        ok,
        tests,
        violations,
    })
}

pub fn resolve_worktree(explicit: Option<String>) -> PathBuf {
    explicit
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap())
}
