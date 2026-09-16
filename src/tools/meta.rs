//! The meta tools: plan_tracker (with loop checkpoints at step boundaries),
//! verify (structured pass/fail), ctx_search (the knowledge drive), and the
//! terminal handoff.

use super::exec::destructive_guard;
use super::{Tool, ToolCtx};
use crate::llm::{truncate, ToolSchema};
use crate::plan::{self, Step};
use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::process::Command;

pub(super) fn plan_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "plan_tracker",
            "Update the tracked plan (.pheobe/plan.json): the full step list with status todo|doing|done|blocked and doubts.",
            json!({"type":"object","properties":{"steps":{"type":"array","items":{"type":"object","properties":{"desc":{"type":"string"},"status":{"type":"string"},"doubts":{"type":"array","items":{"type":"string"}}},"required":["desc","status"]}}},"required":["steps"]}),
        ),
        handler: Box::new(move |c, a| {
            let steps: Vec<Step> = serde_json::from_value(a["steps"].clone())?;
            if let Err(why) = plan::validate_steps(&steps) {
                bail!("plan_tracker refused: {why}");
            }
            let prev = plan::load(c.wt).ok().flatten();
            let p = plan::Plan { task: c.task.task.clone(), steps };
            checkpoint_step_boundaries(c, &p, prev.as_ref());
            plan::save(c.wt, &p)?;
            Ok("plan updated".into())
        }),
    }
}

/// Loop checkpoints at plan-step boundaries (PHEOBE-10): each step newly
/// marked done gets a named snapshot of the current dirty state. Best-effort
/// and tree-safe — a failed snapshot never fails the plan update.
fn checkpoint_step_boundaries(ctx: &ToolCtx<'_>, new: &plan::Plan, old: Option<&plan::Plan>) {
    for (i, s) in new.steps.iter().enumerate() {
        if s.status != "done" {
            continue;
        }
        let was_done = old
            .and_then(|o| o.steps.get(i))
            .map(|ps| ps.status == "done")
            .unwrap_or(false);
        if !was_done {
            let _ = crate::checkpoint::create(ctx.wt, &format!("step-{i}"));
        }
    }
}

pub(super) fn verify_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "verify",
            "Run a command and return structured pass/fail. Test-runner output (cargo/jest/pytest/go) is parsed into structured evidence {runner,total,passed,failed,failures}; the raw excerpt stays as fallback.",
            json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}),
        ),
        handler: Box::new(move |c, a| {
            let cmd = a["command"].as_str().context("command required")?;
            if let Some(why) = destructive_guard(cmd) {
                bail!("safe-exec deny: {why}");
            }
            let out = Command::new("sh").args(["-c", cmd]).current_dir(c.wt).output()?;
            let passed = out.status.code().unwrap_or(-1) == 0;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let structured = crate::testparse::parse(&combined)
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
                .unwrap_or(Value::Null);
            Ok(json!({"passed": passed, "excerpt": truncate(combined.trim_end(), 3000), "structured": structured}).to_string())
        }),
    }
}

pub(super) fn ctx_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "ctx_search",
            "Search the knowledge drive (verified, freshness-tracked facts your training may lack). Read the source on disk when no entry exists.",
            json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
        ),
        handler: Box::new(move |c, a| {
            let q = a["query"].as_str().context("query required")?.to_lowercase();
            let entries = crate::knowledge::load_all(Some(c.wt))?;
            let hits: Vec<&crate::knowledge::Entry> = entries
                .iter()
                .filter(|e| e.slug.contains(&q) || e.name.to_lowercase().contains(&q))
                .take(5)
                .collect();
            if hits.is_empty() {
                return Ok(format!("no entry for '{q}' — read the source on disk instead of recalling"));
            }
            Ok(hits.iter().map(|e| format!("{} ({}, v{}, {})", e.slug, e.kind, e.version.clone().unwrap_or("?".into()), if e.stale { "STALE" } else { "fresh" })).collect::<Vec<_>>().join("\n"))
        }),
    }
}

pub(super) fn handoff_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "handoff",
            "TERMINAL tool — end the run by handing off. Provide summary, next_steps, doubts (unverified assumptions). Branch/worktree/commits/tests are filled mechanically.",
            json!({"type":"object","properties":{"ok":{"type":"boolean"},"summary":{"type":"string"},"next_steps":{"type":"array","items":{"type":"string"}},"doubts":{"type":"array","items":{"type":"string"}},"blocked":{"type":"string"}},"required":["ok","summary"]}),
        ),
        handler: Box::new(|_, _| Ok("handoff recorded — the loop ends here".into())),
    }
}
