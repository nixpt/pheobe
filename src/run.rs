//! The run pipeline shared by the CLI (`cmd run`, main.rs) and the ACP
//! server (`cmd acp`, acp.rs): sandbox intake → worktree provision → orient
//! brief → plan seed → the model turn (provider or worker adapter) → the
//! mechanical gates (allowlist, commit) → done_when → report. One loop, no
//! fork. Callers supply a progress callback for stage transitions — the CLI
//! eprintln!'s them, ACP streams them as `session/update` notifications, and
//! the report JSON rides back as the final session message.

use anyhow::{bail, Result};
use std::path::Path;

use crate::{
    agent, aging, knowledge, learn, llm, plan, report, sandbox, structint, task, verify, worker,
    worktree,
};

/// Run the full pipeline for a parsed task and return the handoff report.
/// `branch` overrides the task's branch (and the auto-derived default).
/// `progress` receives each stage-transition line (the CLI prints these to
/// stderr; the ACP server turns them into AgentMessageChunk notifications).
/// Callers own printing the report: `report::emit` for the CLI, the final
/// session message for ACP — stdout is the ACP wire there, so this core
/// never writes it.
pub fn run_task(
    task: &task::Task,
    branch: Option<&str>,
    progress: &dyn Fn(&str),
) -> Result<report::HandoffReport> {
    // sandbox intake gate (PHEOBE-14): an un-deliverable strict tier is a
    // blocked:no_sandbox BEFORE the loop, never a mid-run surprise
    let sandbox_tier_name = task.effective_sandbox()?;
    let sandbox_tier = sandbox::Tier::from_name(&sandbox_tier_name)?;
    sandbox::ensure_at_intake(&sandbox_tier)?;
    let repo = task.resolve_repo()?;
    let branch = branch
        .map(|b| b.to_string())
        .or(task.branch.clone())
        .unwrap_or_else(|| default_branch(&task.task));
    let task_id = task_slug(&task.task);

    if !task.worktree && Path::new(&repo).join(".git").is_dir() {
        bail!(
            "refusing to cook in the primary source checkout — set worktree:true (default) \
             or point repo at a worktree (the source checkout is shared: mom's kitchen rule)"
        );
    }
    let (wt, branch) = worktree::provision(&repo, &branch)?;
    let result = run_after_provision(
        task,
        &wt,
        &branch,
        &task_id,
        &repo,
        &sandbox_tier_name,
        progress,
    );
    if result.is_err() && !worktree::keep_requested() {
        let _ = worktree::teardown(&repo, &wt, &branch);
    }
    result
}

fn run_after_provision(
    task: &task::Task,
    wt: &Path,
    branch: &str,
    task_id: &str,
    repo: &Path,
    sandbox_tier_name: &str,
    progress: &dyn Fn(&str),
) -> Result<report::HandoffReport> {
    let base_sha = worktree::head_sha(wt)?;
    progress(&format!("🍳 worktree: {}  branch: {branch}", wt.display()));
    let repo_str = repo.display().to_string();
    let session = learn::begin_session(repo, &task.task)?;

    // orient: knowledge drive brief (repo-local + global drives) + learned nudges
    let entries = knowledge::load_all(Some(wt))?;
    let mut brief = knowledge::brief(&entries, Some(wt));
    // structural read brief (polydex, fresh index) — empty when absent/stale;
    // skip-don't-fail, same posture as the knowledge drive
    let structural = structint::orient_brief(wt);
    if !structural.is_empty() {
        brief.push_str(&structural);
    }
    // strict-tier note rides in the prompt so the model knows the bash
    // tool is allowlist-gated and network-free
    if sandbox_tier_name == "strict" {
        brief.push_str(sandbox::STRICT_NOTE);
    }
    let nudges = learn::nudges_for(&repo_str);
    if !nudges.is_empty() {
        progress(&format!(
            "📚 {} learned nudge(s) for this repo",
            nudges.len()
        ));
    }

    // plan file seeded; the model refines it through the loop
    plan::save(
        wt,
        &plan::Plan {
            task: task.task.clone(),
            steps: vec![],
        },
    )?;

    // the model turn, dispatched by PHEOBE_PROVIDER (PHEOBE-9):
    //   openai (default)  → the built-in per-turn Provider loop
    //   <worker adapter>  → one prompt out, one whole run back (PHEOBE-4..8)
    // unknown provider → a clear error from the registry, before anything else
    let provider_name = std::env::var("PHEOBE_PROVIDER").unwrap_or_else(|_| "openai".to_string());
    let ttl = task.ttl.as_deref().map(aging::parse_ttl).transpose()?;
    let cfg = agent::LoopCfg {
        max_turns: task
            .budget
            .as_ref()
            .map(|b| b.max_iterations * 8)
            .unwrap_or(60),
        ttl,
        max_usd: task.budget.as_ref().and_then(|b| b.max_usd),
        usd_per_mtok: std::env::var("PHEOBE_USD_PER_MTOK")
            .ok()
            .and_then(|v| v.parse().ok()),
    };
    let outcome = match worker::worker_from_env(&provider_name)? {
        Some(w) => agent::run_worker(w.as_ref(), task, wt, task_id, &brief, &nudges, &cfg)?,
        None => {
            let provider = llm::OpenAi::from_env()?;
            agent::run(&provider, task, wt, task_id, &brief, &nudges, &cfg)?
        }
    };

    // mechanical gates run after the loop and have the final word over the model
    let dirty = worktree::status_dirty(wt)?;
    if outcome.ok && dirty {
        let (violations, byproducts) = worktree::check_allowlist(wt, &task.paths_allow)?;
        if !byproducts.is_empty() {
            progress(&format!(
                "📝 bash-run byproducts (uncommitted, staged out): {}",
                byproducts.join(", ")
            ));
        }
        if !violations.is_empty() {
            let _ = learn::end_session(session.as_ref(), "allowlist_violation", 2);
            let rep = report::HandoffReport::failure(
                &task.task,
                &format!("paths outside paths_allow: {}", violations.join(", ")),
            );
            return Ok(rep);
        }
        worktree::commit(
            wt,
            task_id,
            outcome.summary.as_deref().unwrap_or(&task.task),
            &task.paths_allow,
        )?;
    }
    // everything on the branch since provision — the engine's own commits
    // (worker route) and the gate's (issue 05)
    let commits = worktree::commits_since(wt, &base_sha)?;

    let tests = if outcome.ok || dirty {
        Some(verify::run_done_when(task, wt)?)
    } else {
        None
    };
    let ok = outcome.ok && tests.as_ref().map(|t| t.passed).unwrap_or(true);
    if !ok {
        progress("❌ done_when failed");
    }
    let _ = learn::end_session(
        session.as_ref(),
        if ok { "done" } else { "failed" },
        if ok { 0 } else { 1 },
    );

    let rep = report::HandoffReport {
        ok,
        task: task.task.clone(),
        branch: Some(branch.to_string()),
        worktree: Some(wt.display().to_string()),
        commits,
        tests,
        summary: outcome.summary,
        next_steps: outcome.next_steps,
        doubts: outcome.doubts,
        blocked: if ok {
            None
        } else {
            outcome.blocked.or(Some("done_when failed".into()))
        },
        usage: Some(report::Usage {
            turns: outcome.usage.turns,
            usd: None,
        }),
    };
    if task.push && ok {
        worktree::push(wt, rep.branch.clone().unwrap_or_default().as_str())?;
    }
    Ok(rep)
}

pub(crate) fn default_branch(task: &str) -> String {
    format!("pheobe/{}", task_slug(task))
}

fn task_slug(t: &str) -> String {
    t.chars()
        .take(24)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_lowercase()
}
