//! The codex worker adapter (PHEOBE-7, CLI path): `PHEOBE_PROVIDER=codex`.
//!
//! Decision recorded (ticket PHEOBE-7): the adapter spawns the codex CLI
//! headless — `codex exec` (no approval prompts, the `deny_all` posture) —
//! and parses its `--json` JSONL event stream, rather than driving the
//! Python `openai-codex` SDK over a subprocess script. Same posture as the
//! other worker adapters: one prompt out, one whole run back, no SDK
//! install requirement. The SDK route (thread_start(sandbox=, approval_mode
//! =deny_all, cwd=, base_instructions=) → thread.run(task) → TurnResult) is
//! the recorded follow-up.
//!
//! Mapping onto the DESIGN.md §"Codex + Kimi integration" shape:
//!
//! | pheobe | codex |
//! |--------|-------|
//! | `thread_start(approval_mode=deny_all, cwd=<worktree>)` | `exec --skip-git-repo-check` + the prompt on argv, cwd = worktree |
//! | `thread.run(task)` | the run behind `exec` (the engine owns its loop) |
//! | `TurnResult.final_response` | last `item.completed` agent_message |
//! | `TurnResult` token usage | `turn.completed` usage (input + output + reasoning; cached input is a subset of input) |
//! | `Sandbox.read_only/workspace_write/full_access` | the tier ladder below |
//!
//! Tier ladder (`PHEOBE_SANDBOX`, default moderate; unknown tier = error at
//! construction, never a silent downgrade):
//!
//! | tier | `--sandbox` |
//! |------|-------------|
//! | strict | `read-only` — allowlist note appended to the prompt; pheobe's own mechanical gates (paths_allow at commit, done_when) stay the real allowlist |
//! | moderate (default) | `workspace-write` |
//! | free | `danger-full-access` — pheobe's policy invariants still hold around the run (no protected branches, paths_allow at commit) |
//!
//! The binary comes from `PHEOBE_CODEX_BIN` (default `codex` on PATH).

use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

use crate::worker::{Worker, WorkerOutcome};

#[derive(Debug)]
pub struct CodexWorker {
    bin: String,
    tier: String,
}

impl CodexWorker {
    pub fn from_env() -> Result<Self> {
        let bin = std::env::var("PHEOBE_CODEX_BIN").unwrap_or_else(|_| "codex".into());
        let tier = std::env::var("PHEOBE_SANDBOX").unwrap_or_else(|_| "moderate".into());
        let tier = tier.trim().to_ascii_lowercase();
        // validate the tier at construction: an unknown PHEOBE_SANDBOX is a
        // config error, not a mid-run surprise
        sandbox_flag(&tier)?;
        Ok(CodexWorker { bin, tier })
    }
}

/// `PHEOBE_SANDBOX` tier → codex `--sandbox` mode (the preset ladder).
fn sandbox_flag(tier: &str) -> Result<&'static str> {
    match tier {
        "strict" => Ok("read-only"),
        "moderate" => Ok("workspace-write"),
        "free" => Ok("danger-full-access"),
        other => bail!(
            "unknown PHEOBE_SANDBOX tier '{other}' — codex adapter expects \
             strict | moderate | free (defaults to moderate when unset)"
        ),
    }
}

/// The strict-tier allowlist note: codex itself runs read-only, so the
/// model must know the allowlist is enforced by pheobe's gates after the run.
const STRICT_NOTE: &str = "\n\n[sandbox note: strict tier runs codex in a read-only sandbox. \
     pheobe's paths_allow gates enforce the allowlist mechanically after this run; \
     a task that must write files or commit needs PHEOBE_SANDBOX=moderate.]";

/// The worker-shape note: the loop prompt's report contract asks for a
/// `handoff` TOOL call, which codex has no analogue for — so the adapter
/// re-states the contract as the final-message contract (one JSON object,
/// exactly the same shape). `parse_jsonl` then lifts it into `json_tail`.
const WORKER_NOTE: &str = "\n\n[worker note: you have no `handoff` tool. End the run with your \
     final message being exactly the report object itself, nothing else: \
     {\"ok\":bool,\"summary\":\"what changed\",\"next_steps\":[...],\"doubts\":[...],\
     \"blocked\":\"reason if blocked\"}.]";

fn registry_worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(CodexWorker::from_env()?))
}

/// Registry entry hook (`worker.rs` REGISTRY): the `WorkerFactory` shape.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    registry_worker()
}

impl Worker for CodexWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        let sandbox = sandbox_flag(&self.tier)?;
        let mut prompt = format!("{prompt}{WORKER_NOTE}");
        if self.tier == "strict" {
            prompt.push_str(STRICT_NOTE);
        }
        let out = Command::new(&self.bin)
            .args(["exec", "--json", "--skip-git-repo-check", "--sandbox", sandbox])
            .arg(&prompt)
            .current_dir(worktree)
            .output()
            .with_context(|| {
                format!(
                    "failed to spawn '{} exec' — is the codex binary on PATH \
                     (or point PHEOBE_CODEX_BIN at it)?",
                    self.bin
                )
            })?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            bail!(
                "codex exec exited with {}: {}",
                out.status,
                stderr.trim()
            );
        }
        parse_jsonl(&String::from_utf8_lossy(&out.stdout))
    }
}

/// The `--json` event stream: last `agent_message` is the final text, the
/// last `turn.completed` usage is the token count. A final message that is
/// itself a report-contract object (`ok`/`summary`) becomes the `json_tail`.
fn parse_jsonl(stdout: &str) -> Result<WorkerOutcome> {
    let mut final_text: Option<String> = None;
    let mut tokens: Option<u64> = None;
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { continue };
        match v.get("type").and_then(|t| t.as_str()) {
            Some("item.completed") => {
                let item = &v["item"];
                if item.get("type").and_then(|t| t.as_str()) == Some("agent_message") {
                    final_text = item.get("text").and_then(|t| t.as_str()).map(String::from);
                }
            }
            Some("turn.completed") => {
                if let Some(usage) = v.get("usage") {
                    let sum: u64 = ["input_tokens", "output_tokens", "reasoning_output_tokens"]
                        .iter()
                        .filter_map(|k| usage.get(*k).and_then(|n| n.as_u64()))
                        .sum();
                    if sum > 0 {
                        tokens = Some(sum);
                    }
                }
            }
            _ => {}
        }
    }
    let final_text = final_text.ok_or_else(|| {
        anyhow::anyhow!(
            "codex exec produced no agent_message — raw output: {}",
            truncate(stdout)
        )
    })?;
    let trimmed = final_text.trim();
    let json_tail = if trimmed.starts_with('{') {
        let mut tail = serde_json::from_str::<Value>(trimmed).ok();
        // an empty "blocked" is the model stating "nothing blocks me" — a
        // non-block, not a block (agent::normalize honors presence)
        if let Some(t) = &mut tail {
            if t.get("blocked").and_then(|b| b.as_str()) == Some("") {
                t.as_object_mut().unwrap().remove("blocked");
            }
        }
        tail.filter(|v| v.get("ok").is_some() || v.get("summary").is_some())
    } else {
        None
    };
    Ok(WorkerOutcome { final_text, tokens, usd: None, json_tail })
}

fn truncate(s: &str) -> &str {
    if s.len() <= 400 {
        s
    } else {
        let mut end = 400;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        &s[..end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("pheobe-codex-{name}-{}-{}", std::process::id(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_codex(dir: &Path, name: &str, body: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
        }
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    fn outcome_dir(dir: &Path) -> WorkerOutcome {
        // a canned --json event stream, the shape the real binary prints
        let script = fake_codex(dir, "codex", r#"cat <<'EOF'
{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"fixed the parser and reran the suite"}}
{"type":"item.completed","item":{"id":"item_1","type":"agent_message","text":"done — parser fixed, suite green"}}
{"type":"turn.completed","usage":{"input_tokens":100,"cached_input_tokens":50,"cache_write_input_tokens":0,"output_tokens":7,"reasoning_output_tokens":3}}
EOF
"#);
        let worker = CodexWorker { bin: script.to_string_lossy().into(), tier: "moderate".into() };
        worker.run("the prompt", dir).unwrap()
    }

    /// A fake codex that appends one line per argv element (plus `CWD=`) to
    /// `capture`, then prints a canned `--json` stream for `final`.
    fn capture_codex(dir: &Path, final_text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let capture = dir.join("captured.txt");
        let json = serde_json::json!({
            "type": "item.completed",
            "item": { "id": "item_0", "type": "agent_message", "text": final_text }
        });
        let body = format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'\ndone\nprintf '%s\\n' \"CWD=$(pwd)\" >> '{}'\ncat <<'EOF'\n{json}\nEOF\n",
            capture.display(),
            capture.display()
        );
        let script = fake_codex(dir, "codex", &body);
        (script, capture)
    }

    #[test]
    fn codex_sandbox_tier_maps_to_the_cli_flag_ladder() {
        assert_eq!(sandbox_flag("strict").unwrap(), "read-only");
        assert_eq!(sandbox_flag("moderate").unwrap(), "workspace-write");
        assert_eq!(sandbox_flag("free").unwrap(), "danger-full-access");
        let err = format!("{:#}", sandbox_flag("chaos").unwrap_err());
        assert!(err.contains("unknown PHEOBE_SANDBOX tier 'chaos'"), "got: {err}");
        assert!(err.contains("strict | moderate | free"), "got: {err}");
    }

    #[test]
    fn codex_final_text_and_tokens_extracted_from_the_jsonl_tail() {
        let dir = scratch("jsonl");
        let out = outcome_dir(&dir);
        assert_eq!(out.final_text, "done — parser fixed, suite green", "last agent_message wins");
        assert_eq!(out.tokens, Some(110), "input + output + reasoning, cached input is a subset");
        assert_eq!(out.usd, None, "codex reports no USD cost");
        assert_eq!(out.json_tail, None, "prose is prose");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn codex_argv_shape_cwd_and_tier_reach_the_binary() {
        let dir = scratch("argv");
        let (script, capture) = capture_codex(&dir, "ok");
        let worker = CodexWorker { bin: script.to_string_lossy().into(), tier: "moderate".into() };
        let out = worker.run("do the thing\ndone_when: `cargo test`", &dir).unwrap();
        assert_eq!(out.final_text, "ok");
        let captured = std::fs::read_to_string(&capture).unwrap();
        let lines: Vec<&str> = captured.lines().collect();
        assert_eq!(lines[0], "exec");
        assert_eq!(lines[1], "--json");
        assert_eq!(lines[2], "--skip-git-repo-check");
        assert_eq!(lines[3], "--sandbox");
        assert_eq!(lines[4], "workspace-write", "default tier is moderate");
        assert!(captured.contains("done_when"), "the whole prompt is one argv element");
        assert!(
            captured.contains("no `handoff` tool"),
            "the final-message report contract rides along, got: {captured}"
        );
        assert_eq!(lines[lines.len() - 1], &format!("CWD={}", dir.display()), "cwd = worktree");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn codex_strict_tier_runs_read_only_and_carries_the_allowlist_note() {
        let dir = scratch("strict");
        let (script, capture) = capture_codex(&dir, "ok");
        let worker = CodexWorker { bin: script.to_string_lossy().into(), tier: "strict".into() };
        worker.run("analyze only", &dir).unwrap();
        let captured = std::fs::read_to_string(&capture).unwrap();
        assert!(captured.contains("--sandbox\nread-only"), "strict = read-only, got: {captured}");
        assert!(
            captured.contains("read-only sandbox"),
            "the allowlist note rides along, got: {captured}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn codex_missing_binary_errors_clearly() {
        let dir = scratch("missing");
        let worker = CodexWorker { bin: "/nonexistent/pheobe-fake-codex".into(), tier: "moderate".into() };
        let err = format!("{:#}", worker.run("x", &dir).unwrap_err());
        assert!(err.contains("failed to spawn '/nonexistent/pheobe-fake-codex exec'"), "got: {err}");
        assert!(err.contains("PHEOBE_CODEX_BIN"), "names the env override, got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn codex_failed_run_surfaces_the_stderr_tail() {
        let dir = scratch("failed");
        let script = fake_codex(dir.as_path(), "codex", "echo 'nope: bad config' >&2; exit 1");
        let worker = CodexWorker { bin: script.to_string_lossy().into(), tier: "moderate".into() };
        let err = format!("{:#}", worker.run("x", &dir).unwrap_err());
        assert!(err.contains("exited with"), "got: {err}");
        assert!(err.contains("nope: bad config"), "stderr tail included, got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn codex_json_report_contract_becomes_the_json_tail() {
        let dir = scratch("tail");
        let script = fake_codex(dir.as_path(), "codex", r#"cat <<'EOF'
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"{\"ok\":true,\"summary\":\"landed the fix\",\"next_steps\":[\"merge\"],\"doubts\":[]}"}}
{"type":"turn.completed","usage":{"input_tokens":5,"output_tokens":5}}
EOF
"#);
        let worker = CodexWorker { bin: script.to_string_lossy().into(), tier: "moderate".into() };
        let out = worker.run("x", &dir).unwrap();
        let tail = out.json_tail.expect("report-contract object becomes the tail");
        assert_eq!(tail["summary"], "landed the fix");
        assert_eq!(tail["ok"], true);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn codex_empty_blocked_in_the_tail_is_not_a_block() {
        let dir = scratch("empty-blocked");
        let script = fake_codex(dir.as_path(), "codex", r#"cat <<'EOF'
{"type":"item.completed","item":{"id":"item_0","type":"agent_message","text":"{\"ok\":true,\"summary\":\"landed\",\"blocked\":\"\"}"}}
EOF
"#);
        let worker = CodexWorker { bin: script.to_string_lossy().into(), tier: "moderate".into() };
        let out = worker.run("x", &dir).unwrap();
        let tail = out.json_tail.expect("report-contract object becomes the tail");
        assert!(tail.get("blocked").is_none(), "empty blocked is a non-block, got: {tail}");
        assert_eq!(tail["ok"], true);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn codex_registry_resolves_and_unknown_tier_is_a_config_error() {
        // env juggling is racy under parallel tests — serialize this one
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = ENV_LOCK.lock().unwrap();
        let dir = scratch("registry");
        let script = fake_codex(dir.as_path(), "codex", r#"echo '{"type":"item.completed","item":{"id":"i","type":"agent_message","text":"ok"}}'"#);
        std::env::set_var("PHEOBE_CODEX_BIN", script.to_string_lossy().into_owned());
        std::env::set_var("PHEOBE_SANDBOX", "chaos");
        let err = format!("{:#}", CodexWorker::from_env().unwrap_err());
        assert!(err.contains("unknown PHEOBE_SANDBOX tier 'chaos'"), "got: {err}");
        std::env::remove_var("PHEOBE_SANDBOX");
        let w = CodexWorker::from_env().unwrap();
        assert_eq!(w.tier, "moderate", "unset env defaults to moderate");
        std::env::remove_var("PHEOBE_CODEX_BIN");
        let w = CodexWorker::from_env().unwrap();
        assert_eq!(w.bin, "codex", "unset bin falls back to PATH lookup");
        std::fs::remove_dir_all(&dir).ok();
    }
}
