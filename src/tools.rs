//! The tool barn — v0.2 working subset. Handlers are sync; the barn is the
//! machine-checked body of the loop (Hopper: the machine checks everything).

use crate::llm::{truncate, ToolSchema};
use crate::plan::{self, Step};
use crate::task::Task;
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Command;

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
        read_tool(ctx),
        write_tool(ctx),
        edit_tool(ctx),
        glob_tool(ctx),
        grep_tool(ctx),
        bash_tool(ctx),
        plan_tool(ctx),
        verify_tool(ctx),
        ctx_tool(ctx),
        handoff_tool(ctx),
    ];
    t.extend(structural_tools(ctx));
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

/// jokersquad safe-exec semantics: hard-deny destructive patterns, modes
/// warn/deny/off (default deny). Exit 99 on hard-deny is upstream's shape.
fn destructive_guard(cmd: &str) -> Option<&'static str> {
    const DENY: &[(&str, &str)] = &[
        ("rm -rf /", "rm -rf /"),
        ("rm -rf ~", "rm -rf ~"),
        ("rm -rf .", "rm -rf ."),
        ("git push --force", "git push --force"),
        ("git push -f", "git push -f"),
        ("git reset --hard", "git reset --hard"),
        ("git checkout .", "git checkout ."),
        ("sudo ", "sudo"),
        ("dd if=", "dd"),
        ("mkfs", "mkfs"),
        ("shutdown", "shutdown"),
        ("reboot", "reboot"),
        (":(){:|:&", "fork bomb"),
        ("chmod -R 777 /", "chmod -R 777 /"),
    ];
    let lower = cmd.to_lowercase();
    DENY.iter().find(|(p, _)| lower.contains(p)).map(|(_, why)| *why)
}

pub fn run_bash(cmd: &str, cwd: &Path, timeout_note: &str) -> Result<String> {
    run_bash_tiered(cmd, cwd, &crate::sandbox::Tier::Moderate, timeout_note)
}

/// The bash tool's execution path (PHEOBE-14): destructive guard in EVERY
/// tier, then the sandbox ladder (strict = bwrap w/o network + allowlist,
/// moderate = bwrap w/ network, free = plain subprocess).
pub fn run_bash_tiered(
    cmd: &str,
    cwd: &Path,
    tier: &crate::sandbox::Tier,
    timeout_note: &str,
) -> Result<String> {
    let _ = timeout_note;
    match std::env::var("PHEOBE_SAFE_EXEC_MODE").ok().as_deref() {
        Some("off") => {}
        Some("warn") => {
            if let Some(why) = destructive_guard(cmd) {
                eprintln!("safe-exec warn: would deny '{why}' — running anyway");
            }
        }
        _ => {
            if let Some(why) = destructive_guard(cmd) {
                bail!("safe-exec deny: {why} — blocked before exec (PHEOBE_SAFE_EXEC_MODE=warn to allow)")
            }
        }
    }
    // strict tier: only test/build/done_when-style commands pass the allowlist
    if matches!(tier, crate::sandbox::Tier::Strict) && !crate::sandbox::strict_allowed(cmd) {
        bail!(
            "sandbox strict tier: command not on the allowlist ({cmd:?}) — only \
             test/build/done_when prefixes (cargo test, cargo build, npm test, pytest, \
             python3, go test, node, sh -c) are permitted; set PHEOBE_SANDBOX=moderate \
             for general shell"
        );
    }
    // bwrap isolation for strict/moderate; free (or missing bwrap on moderate)
    // runs a plain guarded subprocess
    match crate::sandbox::sandboxed_command(cmd, cwd, tier)? {
        Some(mut child) => {
            let out = child.output()?;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let code = out.status.code().unwrap_or(-1);
            Ok(format!("[sandbox exit {code}]\n{}", truncate(combined.trim_end(), 8000)))
        }
        None => {
            let out = Command::new("sh").args(["-c", cmd]).current_dir(cwd).output()?;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let code = out.status.code().unwrap_or(-1);
            Ok(format!("[exit {code}]\n{}", truncate(combined.trim_end(), 8000)))
        }
    }
}

// ── tools ────────────────────────────────────────────────────────────────────

fn resolve(ctx: &ToolCtx<'_>, path: &str) -> Result<PathBuf> {
    let p = Path::new(path);
    if p.is_absolute() {
        // root jail: absolute paths must sit inside the worktree
        let wt = ctx.wt.canonicalize().unwrap_or_else(|_| ctx.wt.to_path_buf());
        let p = p.canonicalize().unwrap_or_else(|_| p.to_path_buf());
        if !p.starts_with(&wt) {
            bail!("path escapes the worktree root: {path}");
        }
        Ok(p)
    } else {
        Ok(ctx.wt.join(p))
    }
}

fn read_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "read",
            "Read a file (text, size/line-limited). Path is worktree-relative or absolute inside the worktree.",
            json!({"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer","description":"1-based start line"},"limit":{"type":"integer","description":"max lines (default 400)"}},"required":["path"]}),
        ),
        handler: Box::new(move |c, a| {
            let p = resolve(c, a["path"].as_str().context("path required")?)?;
            let txt = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
            let offset = a["offset"].as_u64().unwrap_or(1).max(1) as usize;
            let limit = a["limit"].as_u64().unwrap_or(400) as usize;
            let lines: Vec<&str> = txt.lines().skip(offset - 1).take(limit).collect();
            Ok(format!("{}{}:{}", p.display(), if lines.len() == limit { "+ (truncated)" } else { "" }, lines.join("\n")))
        }),
    }
}

fn write_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "write",
            "Write a file (overwrites). Only inside the worktree and paths_allow.",
            json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}),
        ),
        handler: Box::new(move |c, a| {
            let p = resolve(c, a["path"].as_str().context("path required")?)?;
            check_allow(c, &p)?;
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, a["content"].as_str().context("content required")?)?;
            let note = crate::fmt::format_on_write(c.wt, &p).unwrap_or_default();
            Ok(format!("wrote {} ({} bytes){note}", p.display(), a["content"].as_str().unwrap_or("").len()))
        }),
    }
}

fn edit_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "edit",
            "Exact-string replace in a file. old_string must be unique; ambiguity is an error, never resolved silently.",
            json!({"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}},"required":["path","old_string","new_string"]}),
        ),
        handler: Box::new(move |c, a| {
            let p = resolve(c, a["path"].as_str().context("path required")?)?;
            check_allow(c, &p)?;
            let old = a["old_string"].as_str().context("old_string required")?;
            let new = a["new_string"].as_str().context("new_string required")?;
            let txt = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
            let n = txt.matches(old).count();
            if n == 0 {
                bail!("old_string not found in {} — read the file again", p.display());
            }
            if n > 1 {
                bail!("old_string matches {n} times in {} — ambiguous edit refused; add more context", p.display());
            }
            let updated = txt.replacen(old, new, 1);
            std::fs::write(&p, &updated)?;
            let note = crate::fmt::format_on_write(c.wt, &p).unwrap_or_default();
            Ok(format!("edited {} (+{}-{} chars){note}", p.display(), new.len(), old.len()))
        }),
    }
}

fn glob_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "glob",
            "List files matching a pattern (e.g. \"src/**/*.rs\"), .gitignore-aware-ish, capped at 200 hits.",
            json!({"type":"object","properties":{"pattern":{"type":"string","description":"suffix match, e.g. .rs or src/"}},"required":["pattern"]}),
        ),
        handler: Box::new(move |c, a| {
            let pat = a["pattern"].as_str().context("pattern required")?;
            let mut hits = vec![];
            walk(c.wt, 0, &mut |p| {
                let rel = p.strip_prefix(c.wt).unwrap_or(p).display().to_string();
                if hits.len() < 200 && (rel.contains(pat) || rel.ends_with(pat) || pat == "*") {
                    hits.push(rel);
                }
                hits.len() < 200
            });
            hits.sort();
            Ok(if hits.is_empty() { "no matches".into() } else { hits.join("\n") })
        }),
    }
}

fn walk(dir: &Path, depth: usize, f: &mut dyn FnMut(&Path) -> bool) -> bool {
    if depth > 12 {
        return true;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return true };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == ".git" || name.starts_with("target") || name == "node_modules" {
            continue;
        }
        if p.is_dir() {
            if !walk(&p, depth + 1, f) {
                return false;
            }
        } else if !f(&p) {
            return false;
        }
    }
    true
}

fn grep_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "grep",
            "Search file contents for a pattern (plain text, case-insensitive). Capped.",
            json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string","description":"subdir to search (default whole worktree)"}},"required":["pattern"]}),
        ),
        handler: Box::new(move |c, a| {
            let pat = a["pattern"].as_str().context("pattern required")?.to_lowercase();
            let root = match a["path"].as_str() {
                Some(p) => resolve(c, p)?,
                None => c.wt.to_path_buf(),
            };
            let mut out = vec![];
            walk(&root, 0, &mut |p| {
                if out.len() >= 80 {
                    return false;
                }
                if let Ok(txt) = std::fs::read_to_string(p) {
                    if p.extension().map(|e| e == "rs" || e == "md" || e == "toml" || e == "json" || e == "py" || e == "ts" || e == "js" || e == "go" || e == "c" || e == "h").unwrap_or(false) {
                        for (i, line) in txt.lines().enumerate() {
                            if line.to_lowercase().contains(&pat) {
                                out.push(format!("{}:{}: {}", p.strip_prefix(c.wt).unwrap_or(p).display(), i + 1, truncate(line.trim(), 160)));
                                break; // one hit per file keeps the tool result small
                            }
                        }
                    }
                }
                out.len() < 80
            });
            Ok(if out.is_empty() { "no matches".into() } else { out.join("\n") })
        }),
    }
}

fn bash_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "bash",
            "Run a shell command in the worktree. Output truncated to 8KB. Destructive commands are blocked (safe-exec). Commands run sandboxed per the run's tier (strict = no network + allowlist).",
            json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}),
        ),
        handler: Box::new(move |c, a| {
            let cmd = a["command"].as_str().context("command required")?;
            let tier = crate::sandbox::Tier::from_name(&c.task.effective_sandbox()?)?;
            run_bash_tiered(cmd, c.wt, &tier, "bash")
        }),
    }
}

fn plan_tool(_ctx: &ToolCtx<'_>) -> Tool {
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
        let was_done = old.and_then(|o| o.steps.get(i)).map(|ps| ps.status == "done").unwrap_or(false);
        if !was_done {
            let _ = crate::checkpoint::create(ctx.wt, &format!("step-{i}"));
        }
    }
}

fn verify_tool(_ctx: &ToolCtx<'_>) -> Tool {
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

fn ctx_tool(_ctx: &ToolCtx<'_>) -> Tool {
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

fn handoff_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "handoff",
            "TERMINAL tool — end the run by handing off. Provide summary, next_steps, doubts (unverified assumptions). Branch/worktree/commits/tests are filled mechanically.",
            json!({"type":"object","properties":{"ok":{"type":"boolean"},"summary":{"type":"string"},"next_steps":{"type":"array","items":{"type":"string"}},"doubts":{"type":"array","items":{"type":"string"}},"blocked":{"type":"string"}},"required":["ok","summary"]}),
        ),
        handler: Box::new(|_, _| Ok("handoff recorded — the loop ends here".into())),
    }
}

/// The optional structural-ladder tools (PHEOBE-11): registered only when
/// their backend binary is on PATH at barn build time — absent = skipped,
/// never failed. The read ladder upgrades text coordinates to structural
/// ones (skeleton/enclosing/callers/impact/affected-tests). The code-atlas
/// write ladder is gate-ready: `atlas_edit` registers only when code-atlas
/// exists; on this box it is design-only, so the ladder stays empty and the
/// honest fallback (exact-string `edit`) is what models call.
fn structural_tools(_ctx: &ToolCtx<'_>) -> Vec<Tool> {
    let mut t = Vec::new();
    if crate::structint::polydex_bin().is_some() {
        t.push(sym_tool(
            "sym_skeleton",
            "Signatures-only view of one file from the polydex index (symbols, kinds, parents, lines) — the cheap structural read before opening the whole file. Stale index → fall back to `read` and record the doubt.",
            json!({"type":"object","properties":{"file":{"type":"string"}},"required":["file"]}),
            |c, a| crate::structint::skeleton(c.wt, a["file"].as_str().context("file required")?),
        ));
        t.push(sym_tool(
            "sym_callers",
            "Direct callers of a symbol from the polydex call graph (incoming calls edges). Stale index → fall back to `grep` and record the doubt.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
            |c, a| crate::structint::callers(c.wt, a["name"].as_str().context("name required")?),
        ));
        t.push(sym_tool(
            "sym_impact",
            "Transitive callers of a symbol — what breaks if I change this. Stale index → fall back to `grep` and record the doubt.",
            json!({"type":"object","properties":{"symbol":{"type":"string"}},"required":["symbol"]}),
            |c, a| crate::structint::impact(c.wt, a["symbol"].as_str().context("symbol required")?),
        ));
        t.push(sym_tool(
            "sym_affected_tests",
            "Test symbols transitively reachable from the changed symbols — the preferred pre-check before running the suite. Stale index → fall back to `verify` with the whole suite and record the doubt.",
            json!({"type":"object","properties":{"changed":{"type":"array","items":{"type":"string"}}},"required":["changed"]}),
            |c, a| {
                let changed: Vec<String> = a["changed"]
                    .as_array()
                    .context("changed required")?
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                crate::structint::affected_tests(c.wt, &changed)
            },
        ));
    }
    if crate::structint::code_atlas_bin().is_some() {
        t.push(sym_tool(
            "atlas_edit",
            "code-atlas structural edit: stable region handle + two-hash guard + parse-gated atomic write, ALWAYS returns the diff. Ambiguity is an error: report blocked rather than guess. Falls back to `edit` semantics if the region is not uniquely resolvable.",
            json!({"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["path","old","new"]}),
            |c, a| {
                let bin = crate::structint::code_atlas_bin().context("code-atlas unavailable")?;
                let out = std::process::Command::new(&bin)
                    .args(["edit", "--file", a["path"].as_str().context("path required")?])
                    .args(["--old", a["old"].as_str().context("old required")?])
                    .args(["--new", a["new"].as_str().context("new required")?])
                    .current_dir(c.wt)
                    .output()
                    .context("code-atlas edit failed to spawn")?;
                if !out.status.success() {
                    bail!(
                        "atlas_edit exited with {}: {}",
                        out.status,
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                }
                Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
            },
        ));
    }
    t
}

/// Build one structural tool: the freshness gate lives in
/// `structint::read_fresh`, which returns `Ok(None)` only when the backend
/// is absent (the tool should not have been registered) and surfaces the
/// stale-index note as its own result so the model records the doubt.
fn sym_tool(
    name: &str,
    description: &str,
    parameters: Value,
    run: impl for<'a> Fn(&ToolCtx<'a>, &Value) -> anyhow::Result<Option<String>> + Send + Sync + 'static,
) -> Tool {
    let owned_name = name.to_string();
    Tool {
        schema: ToolSchema::new(name, description, parameters),
        handler: Box::new(move |c, a| {
            match run(c, a) {
                Ok(Some(text)) => Ok(text),
                Ok(None) => bail!("{owned_name}: backend unavailable — use the text-tool fallback"),
                Err(e) => Err(e),
            }
        }),
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
