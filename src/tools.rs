//! The tool barn — v0.2 working subset. Handlers are sync; the barn is the
//! machine-checked body of the loop (Hopper: the machine checks everything).
//!
//! Split by tool family (PHEOBE-21 LOC budget): the root owns the shared
//! surface (ctx type, registry, dispatch, the path gate); each family lives
//! in its own module — `fs` (read/write/edit), `search` (glob/grep),
//! `exec` (bash + the safe-exec guard), `meta` (plan/verify/ctx/handoff),
//! `structural` (the polydex ladder).

mod exec;
mod fs;
mod meta;
mod search;
mod structural;

use crate::llm::ToolSchema;
use crate::task::Task;
use anyhow::{bail, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct ToolCtx<'a> {
    pub wt: &'a Path,
    pub task: &'a Task,
    pub task_id: &'a str,
}

type Handler = Box<dyn Fn(&ToolCtx<'_>, &Value) -> Result<String>>;

pub struct Tool {
    pub schema: ToolSchema,
    pub handler: Handler,
}

pub fn barn<'a>(ctx: &'a ToolCtx<'a>) -> Vec<Tool> {
    let mut t = vec![
        fs::read_tool(ctx),
        fs::write_tool(ctx),
        fs::edit_tool(ctx),
        search::glob_tool(ctx),
        search::grep_tool(ctx),
        exec::bash_tool(ctx),
        meta::plan_tool(ctx),
        meta::verify_tool(ctx),
        meta::ctx_tool(ctx),
        meta::handoff_tool(ctx),
    ];
    t.extend(structural::structural_tools(ctx));
    t
}

pub fn schemas(tools: &[Tool]) -> Vec<ToolSchema> {
    tools.iter().map(|t| t.schema.clone()).collect()
}

pub fn dispatch(tools: &[Tool], ctx: &ToolCtx<'_>, name: &str, args: &Value) -> Result<String> {
    for t in tools {
        if t.schema.name() == name {
            return (t.handler)(ctx, args);
        }
    }
    bail!("unknown tool: {name}")
}

// ── shared path gate (worktree jail + paths_allow) ──────────────────────────

fn resolve(ctx: &ToolCtx<'_>, path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() {
        // root jail: absolute paths must sit inside the worktree
        let wt = ctx
            .wt
            .canonicalize()
            .unwrap_or_else(|_| ctx.wt.to_path_buf());
        let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        if !p.starts_with(&wt) {
            bail!("path escapes the worktree root: {path}");
        }
        Ok(p)
    } else {
        Ok(ctx.wt.join(p))
    }
}

fn check_allow(ctx: &ToolCtx<'_>, p: &Path) -> Result<()> {
    if ctx.task.paths_allow.is_empty() {
        return Ok(());
    }
    let rel = p.strip_prefix(ctx.wt).unwrap_or(p).display().to_string();
    let inside = ctx.task.paths_allow.iter().any(|a| {
        let a = a.trim_end_matches('/');
        rel == a || rel.starts_with(&format!("{a}/")) || rel.starts_with(a)
    });
    if !inside {
        bail!("outside paths_allow: {rel} — scope is a contract");
    }
    Ok(())
}
