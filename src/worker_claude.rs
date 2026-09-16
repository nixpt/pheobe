//! The claude worker adapter (PHEOBE-5): `PHEOBE_PROVIDER=claude`.
//!
//! One prompt out, one whole claude run back:
//! `claude -p <prompt> --output-format json <flags>` spawned with
//! cwd = the worktree (the mayfly adapter precedent, upgraded from
//! `--output-format text` to json — the Agent SDK's official subprocess
//! escape hatch — so we get usage + a structured result, not just prose).
//!
//! Report normalization is identical to the opencode adapter: claude's
//! output is treated as prose (the `result` string of its final JSON
//! object); pheobe's mechanical gates own the contract. Engine JSON merges
//! in only through `json_tail` — if the engine's final message itself
//! parses as a JSON object (the model chose to emit the handoff contract),
//! it is handed up and `agent::run_worker` merges ONLY the
//! summary/next_steps/doubts/blocked/ok keys out of it; branch/commits/
//! tests stay mechanical.
//!
//! Env:
//! - `PHEOBE_CLAUDE_BIN` (default `claude`)
//! - `PHEOBE_CLAUDE_FLAGS` — default `--dangerously-skip-permissions`
//!   (split on whitespace). Setting it REPLACES the default entirely:
//!   e.g. `PHEOBE_CLAUDE_FLAGS=ccf-mode` appends nothing else and expects
//!   the caller's env to carry the flownet auth (same pattern as mayfly's
//!   ccf harness: the wrapper env decides the credentials).
//! - `PHEOBE_CLAUDE_TIMEOUT_SECS` (default 3600) — the adapter's own
//!   subprocess timeout: a hung claude is killed here. pheobe's aging
//!   ladder (agent::run_worker) applies its own expiry judgment around the
//!   whole call afterwards; to make the child die AT the ttl, set this
//!   timeout ≤ the task ttl.

use crate::worker::{Worker, WorkerOutcome};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

const DEFAULT_FLAGS: &str = "--dangerously-skip-permissions";
const DEFAULT_TIMEOUT_SECS: u64 = 3600;

/// Registry constructor for `worker::REGISTRY`.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(ClaudeWorker))
}

/// The claude adapter. Mockable at the `Worker` seam like any other.
pub struct ClaudeWorker;

impl Worker for ClaudeWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string());
        let flags =
            std::env::var("PHEOBE_CLAUDE_FLAGS").unwrap_or_else(|_| DEFAULT_FLAGS.to_string());
        let timeout = std::env::var("PHEOBE_CLAUDE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let mut cmd = Command::new(&bin);
        cmd.arg("-p")
            .arg(prompt)
            .arg("--output-format")
            .arg("json");
        cmd.args(flags.split_whitespace());
        cmd.current_dir(worktree);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "claude worker: failed to spawn '{bin}' (is the claude binary on PATH? \
                 set PHEOBE_CLAUDE_BIN to override)"
            )
        })?;

        // drain pipes on threads — a piped claude must never block on a full
        // stdout buffer while we poll
        let mut out_pipe = child.stdout.take().context("claude worker: no stdout pipe")?;
        let mut err_pipe = child.stderr.take().context("claude worker: no stderr pipe")?;
        let stdout_reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = out_pipe.read_to_string(&mut buf);
            buf
        });
        let stderr_reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err_pipe.read_to_string(&mut buf);
            buf
        });

        let status = wait_timeout::ChildExt::wait_timeout(&mut child, Duration::from_secs(timeout))
            .with_context(|| format!("claude worker: waiting on '{bin}' failed"))?
            .with_context(|| {
                format!(
                    "claude worker: '{bin}' timed out after {timeout}s and was killed \
                     (set PHEOBE_CLAUDE_TIMEOUT_SECS to adjust; the aging ladder in \
                     run_worker judges the run separately)"
                )
            })?;

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        if !status.success() {
            anyhow::bail!(
                "claude worker: '{bin}' exited with {status}: {}",
                stderr_tail(&stderr)
            );
        }

        let (outcome, is_error) = parse_stdout(&stdout);
        if is_error {
            anyhow::bail!(
                "claude worker: the run reported is_error: {}",
                text_tail(&outcome.final_text)
            );
        }
        Ok(outcome)
    }
}

/// Token total for the budget estimator: the sum of the usage buckets claude
/// reports on its result object (input + output + both cache classes).
fn sum_usage(usage: &Value) -> Option<u64> {
    const KEYS: [&str; 4] = [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ];
    let mut total = 0u64;
    let mut any = false;
    for key in KEYS {
        if let Some(v) = usage.get(key).and_then(|v| v.as_u64()) {
            total += v;
            any = true;
        }
    }
    any.then_some(total)
}

/// Parse claude's stdout defensively: with `--output-format json` it is a
/// single result object; a stream of objects (one per line) is tolerated;
/// an unparseable tail degrades to "the whole stdout is the engine's prose".
fn parse_stdout(raw: &str) -> (WorkerOutcome, bool) {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            return from_result_object(&v);
        }
        let mut last: Option<Value> = None;
        let mut last_result: Option<Value> = None;
        for line in trimmed.lines() {
            let line = line.trim();
            if !line.starts_with('{') {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                if v.get("type").and_then(|t| t.as_str()) == Some("result") {
                    last_result = Some(v.clone());
                }
                last = Some(v);
            }
        }
        if let Some(v) = last_result.or(last) {
            return from_result_object(&v);
        }
    }
    (
        WorkerOutcome {
            final_text: raw.to_string(),
            tokens: None,
            usd: None,
            json_tail: None,
        },
        false,
    )
}

/// Extract a `WorkerOutcome` from a claude result JSON object. `is_error` is
/// reported separately so the caller can surface it as a hard error.
fn from_result_object(v: &Value) -> (WorkerOutcome, bool) {
    let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
    let final_text = v
        .get("result")
        .map(|r| r.as_str().map(|s| s.to_string()).unwrap_or_else(|| r.to_string()))
        .unwrap_or_default();
    let tokens = v.get("usage").and_then(sum_usage);
    let usd = v.get("total_cost_usd").and_then(|c| c.as_f64());
    // the engine's final message may itself be the handoff contract in JSON —
    // if it parses as an object, hand it up as json_tail (only contract keys
    // merge; see agent::run_worker)
    let json_tail = serde_json::from_str::<Value>(&final_text)
        .ok()
        .filter(|j| j.is_object());
    (
        WorkerOutcome { final_text, tokens, usd, json_tail },
        is_error,
    )
}

fn stderr_tail(stderr: &str) -> String {
    let t = stderr.trim();
    if t.is_empty() {
        "(no stderr)".to_string()
    } else {
        text_tail(t)
    }
}

fn text_tail(text: &str) -> String {
    const MAX: usize = 400;
    if text.chars().count() <= MAX {
        text.to_string()
    } else {
        let cut: String = text.chars().take(MAX).collect();
        format!("{cut}…")
    }
}
