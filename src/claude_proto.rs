//! The claude CLI's stream-json control protocol (PHEOBE-45): message
//! shapes and the per-tool policy the `claude-sdk` worker answers with.
//! Pure: no I/O, so every decision is unit-testable.
//!
//! Wire contract (what the Claude Agent SDK itself speaks; verified against
//! claude-agent-sdk-python `_internal/query.py` + two live runs, s463):
//! - pheobe → CLI (stdin, one JSON object per line):
//!   `{"type":"control_request","request_id":…,"request":{"subtype":"initialize","hooks":…}}`,
//!   `{"type":"user","message":{"role":"user","content":…},"parent_tool_use_id":null,"session_id":"default"}`,
//!   `{"type":"control_response","response":{"subtype":"success"|"error","request_id":…,…}}`,
//!   `{"type":"control_request",…,"request":{"subtype":"interrupt"}}`.
//! - CLI → pheobe (stdout): `system`, `assistant`, `user`, `result`,
//!   `control_response`, and `control_request` with subtype `can_use_tool`
//!   (tools that need permission: Write/Edit/Bash…) or `hook_callback` (our
//!   PreToolUse hook on reads — read-only tools never raise `can_use_tool`).
//!
//! This is the SDK's internal contract, not a documented public API: the
//! fixture test (`tests/fixtures/claude-stream-json.jsonl`) is the
//! conformance check to re-run when the claude CLI version moves.

use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

/// The PreToolUse hook callback id pheobe registers for reads.
pub const READ_HOOK_ID: &str = "pheobe_read_guard";
/// Read-only tools, path-checked through the PreToolUse hook.
pub const READ_TOOLS: &str = "Read|Grep|Glob|LS|NotebookRead";
/// Tools that change files; allowed only inside the worktree ∩ paths_allow.
const WRITE_TOOLS: &[&str] = &["Write", "Edit", "MultiEdit", "NotebookEdit"];
/// Tools allowed without a path check (bookkeeping, no side effects).
const PLAIN_ALLOW: &[&str] = &["TodoWrite", "BashOutput", "KillBash", "KillShell"];

/// One permission decision, kept for the handoff report.
#[derive(Debug, Clone, PartialEq)]
pub struct Decision {
    pub tool: String,
    pub allow: bool,
    pub reason: String,
}

/// The mechanical policy for one run.
#[derive(Debug, Clone)]
pub struct Policy {
    /// The worktree (the engine's cwd) — all writes must land inside it.
    pub worktree: PathBuf,
    /// Extra read roots (the main checkout, for sibling context).
    pub read_roots: Vec<PathBuf>,
    /// The task's `paths_allow` (worktree-relative prefixes); empty = whole worktree.
    pub paths_allow: Vec<String>,
    /// Extra tool names allowed outright (`PHEOBE_CLAUDE_SDK_ALLOW`).
    pub extra_allow: Vec<String>,
}

/// `a/./b/../c` → `a/c` without touching the filesystem (targets may not exist yet).
pub fn normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

impl Policy {
    fn abs(&self, raw: &str) -> PathBuf {
        let p = Path::new(raw);
        normalize(&if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.worktree.join(p)
        })
    }

    fn within_paths_allow(&self, abs: &Path) -> bool {
        if self.paths_allow.is_empty() {
            return true;
        }
        let rel = abs
            .strip_prefix(&self.worktree)
            .unwrap_or(abs)
            .display()
            .to_string();
        // same semantics as tools::check_allow (the built-in loop's gate)
        self.paths_allow.iter().any(|a| {
            let a = a.trim_end_matches('/');
            rel == a || rel.starts_with(&format!("{a}/")) || rel.starts_with(a)
        })
    }

    fn write_decision(&self, tool: &str, path: Option<&str>) -> Decision {
        let d = |allow: bool, reason: String| Decision {
            tool: tool.to_string(),
            allow,
            reason,
        };
        let Some(raw) = path.filter(|p| !p.is_empty()) else {
            return d(false, format!("{tool} without a target path"));
        };
        let abs = self.abs(raw);
        if !abs.starts_with(&self.worktree) {
            return d(false, format!("{tool} outside the worktree: {raw}"));
        }
        if !self.within_paths_allow(&abs) {
            return d(
                false,
                format!("{tool} outside paths_allow: {raw} — scope is a contract"),
            );
        }
        d(true, format!("{tool} inside scope: {raw}"))
    }

    fn bash_decision(&self, cmd: &str) -> Decision {
        let d = |allow: bool, reason: String| Decision {
            tool: "Bash".into(),
            allow,
            reason,
        };
        if let Some(why) = crate::tools::destructive_guard(cmd) {
            return d(false, format!("safe-exec deny: {why}"));
        }
        let lower = cmd.to_lowercase();
        // pheobe pushes mechanically after its gates (task.push); the engine never does
        for (pat, why) in [
            (
                "git push",
                "git push — pheobe pushes after its gates, not the engine",
            ),
            ("gh pr merge", "merging a PR is not a worker's call"),
            ("gh repo delete", "repo deletion"),
        ] {
            if lower.contains(pat) {
                return d(false, why.into());
            }
        }
        // recursive delete: only relative targets that stay inside the worktree
        if lower.split_whitespace().next() == Some("rm") && lower.contains(" -r") {
            let escapes = cmd
                .split_whitespace()
                .skip(1)
                .filter(|w| !w.starts_with('-'))
                .any(|t| {
                    let a = self.abs(t);
                    t.starts_with('~') || !a.starts_with(&self.worktree) || a == self.worktree
                });
            if escapes {
                return d(false, "recursive rm outside (or of) the worktree".into());
            }
        }
        // writes through the shell: best-effort parse of redirect targets and the
        // destination args of file-writing commands, held to the same scope as
        // Write/Edit. Heuristic by nature — run.rs's post-run paths_allow gate
        // is the mechanical backstop for anything a parse can't see.
        for t in bash_write_targets(cmd) {
            let a = self.abs(&t);
            if !a.starts_with(&self.worktree) {
                return d(
                    false,
                    format!("Bash writes outside the worktree: {t} (in `{cmd}`)"),
                );
            }
            if !self.within_paths_allow(&a) {
                return d(
                    false,
                    format!(
                        "Bash writes outside paths_allow: {t} (in `{cmd}`) — scope is a contract"
                    ),
                );
            }
        }
        d(true, format!("bash screened: `{cmd}`"))
    }

    /// Answer a `can_use_tool` request.
    pub fn can_use_tool(&self, tool: &str, input: &Value) -> Decision {
        let s = |k: &str| input.get(k).and_then(Value::as_str);
        if WRITE_TOOLS.contains(&tool) {
            let path = s("file_path").or_else(|| s("notebook_path"));
            return self.write_decision(tool, path);
        }
        if tool == "Bash" {
            return self.bash_decision(s("command").unwrap_or(""));
        }
        if READ_TOOLS.split('|').any(|t| t == tool) {
            return self.read_decision(tool, input);
        }
        let allowed = PLAIN_ALLOW.contains(&tool) || self.extra_allow.iter().any(|t| t == tool);
        Decision {
            tool: tool.to_string(),
            allow: allowed,
            reason: if allowed {
                "on the allowlist".into()
            } else {
                format!("{tool} is not on the pheobe claude-sdk allowlist (PHEOBE_CLAUDE_SDK_ALLOW extends it)")
            },
        }
    }

    /// Path-check a read (the PreToolUse hook on Read/Grep/Glob/LS/NotebookRead).
    pub fn read_decision(&self, tool: &str, input: &Value) -> Decision {
        let raw = ["file_path", "notebook_path", "path"]
            .iter()
            .find_map(|k| input.get(*k).and_then(Value::as_str))
            .filter(|p| !p.is_empty());
        let Some(raw) = raw else {
            // Grep/Glob without a path search the cwd = the worktree
            return Decision {
                tool: tool.into(),
                allow: true,
                reason: "read in the worktree (no path)".into(),
            };
        };
        let abs = self.abs(raw);
        let ok =
            abs.starts_with(&self.worktree) || self.read_roots.iter().any(|r| abs.starts_with(r));
        Decision {
            tool: tool.into(),
            allow: ok,
            reason: if ok {
                format!("read inside the repo: {raw}")
            } else {
                format!("{tool} outside the worktree/repo: {raw}")
            },
        }
    }
}

/// Paths a shell command would write: `>`/`>>` targets, `tee` args, the last
/// arg of `cp`/`mv`/`install`/`ln`, every arg of `touch`/`mkdir`.
/// Split on `;`, `&&`, `||`, `|` so each simple command is judged alone.
pub fn bash_write_targets(cmd: &str) -> Vec<String> {
    let mut out = Vec::new();
    let skip =
        |t: &str| t.is_empty() || t == "/dev/null" || t.starts_with('&') || t.starts_with('$');
    let normalized = cmd.replace("&&", ";").replace("||", ";").replace('|', ";");
    for simple in normalized.split(';') {
        let words: Vec<String> = simple
            .split_whitespace()
            .map(|w| w.trim_matches(|c| c == '"' || c == '\'').to_string())
            .collect();
        // redirects: `> f`, `>f`, `>> f`, `2> f`
        let mut i = 0;
        while i < words.len() {
            let w = &words[i];
            if let Some(pos) = w.find('>') {
                let rest = w[pos..].trim_start_matches('>');
                let target = if rest.is_empty() {
                    words.get(i + 1).cloned().unwrap_or_default()
                } else {
                    rest.to_string()
                };
                if !skip(&target) {
                    out.push(target);
                }
            }
            i += 1;
        }
        let args: Vec<&String> = words
            .iter()
            .skip(1)
            .filter(|w| !w.starts_with('-') && !w.contains('>'))
            .collect();
        match words.first().map(String::as_str) {
            Some("tee") | Some("touch") | Some("mkdir") => {
                out.extend(args.iter().map(|s| s.to_string()))
            }
            Some("cp") | Some("mv") | Some("install") | Some("ln") if args.len() >= 2 => {
                out.push(args[args.len() - 1].to_string())
            }
            _ => {}
        }
    }
    out.retain(|t| !skip(t));
    out
}

// ── message builders ─────────────────────────────────────────────────────────

pub fn initialize(request_id: &str) -> Value {
    json!({
        "type": "control_request",
        "request_id": request_id,
        "request": {
            "subtype": "initialize",
            "hooks": {
                "PreToolUse": [{"matcher": READ_TOOLS, "hookCallbackIds": [READ_HOOK_ID]}]
            }
        }
    })
}

pub fn user_message(prompt: &str) -> Value {
    json!({
        "type": "user",
        "message": {"role": "user", "content": prompt},
        "parent_tool_use_id": null,
        "session_id": "default"
    })
}

pub fn interrupt(request_id: &str) -> Value {
    json!({"type": "control_request", "request_id": request_id, "request": {"subtype": "interrupt"}})
}

fn success(request_id: &str, response: Value) -> Value {
    json!({"type": "control_response", "response": {"subtype": "success", "request_id": request_id, "response": response}})
}

pub fn error(request_id: &str, message: &str) -> Value {
    json!({"type": "control_response", "response": {"subtype": "error", "request_id": request_id, "error": message}})
}

/// The `can_use_tool` answer for a decision.
pub fn permission_response(request_id: &str, d: &Decision, input: &Value) -> Value {
    if d.allow {
        success(
            request_id,
            json!({"behavior": "allow", "updatedInput": input}),
        )
    } else {
        success(
            request_id,
            json!({"behavior": "deny", "message": format!("pheobe: {}", d.reason)}),
        )
    }
}

/// The `hook_callback` answer for a read decision (allow = no opinion).
pub fn hook_response(request_id: &str, d: &Decision) -> Value {
    if d.allow {
        success(request_id, json!({}))
    } else {
        success(
            request_id,
            json!({"hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": format!("pheobe: {}", d.reason)
            }}),
        )
    }
}

/// What a `result` message says about the run.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunResult {
    pub subtype: String,
    pub is_error: bool,
    pub text: String,
    pub usd: Option<f64>,
    pub turns: Option<u32>,
    pub tokens: Option<u64>,
}

pub fn parse_result(v: &Value) -> RunResult {
    RunResult {
        subtype: v
            .get("subtype")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        text: v
            .get("result")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        usd: v.get("total_cost_usd").and_then(Value::as_f64),
        turns: v.get("num_turns").and_then(Value::as_u64).map(|n| n as u32),
        tokens: v.get("usage").and_then(crate::worker_claude::sum_usage),
    }
}
