//! The cursor worker adapter (PHEOBE-6, sandbox mapping PHEOBE-23):
//! `PHEOBE_PROVIDER=cursor`.
//!
//! CLI path (decision recorded in PHEOBE-6): spawn `cursor-agent`
//! non-interactively — `-p --output-format json` — and parse stdout
//! defensively. Same posture as the other adapters: one prompt out, one
//! whole run back, no Node/TS `@cursor/sdk` shim. `Agent.prompt()` +
//! `getUsage()` + `run.steer()` stay the recorded follow-up (Worker is
//! one-shot; steer needs an in-process `Run`).
//!
//! argv (verified against `cursor-agent --help` on this box):
//! `cursor-agent -p --output-format json --sandbox <mode> [flags] <prompt>`
//! with cwd = the worktree pheobe already provisioned. Do **not** pass
//! `--worktree`: that flag creates a nested tree under
//! `~/.cursor/worktrees/<repo>/`, a second isolation layer pheobe does
//! not own.
//!
//! Flags come from `PHEOBE_CURSOR_FLAGS`, default `--yolo --trust`
//! (mayfly-proven: `--yolo` = `--force`, headless auto-approve; `--trust`
//! = skip the workspace prompt). Setting `PHEOBE_CURSOR_FLAGS` REPLACES
//! the default entirely (claude-adapter convention). `--sandbox` is
//! pheobe policy, not a flag: it is always appended from `PHEOBE_SANDBOX`.
//!
//! Cursor's CLI sandbox is two-valued (`enabled` | `disabled`), matching
//! `@cursor/sdk` `local.sandboxOptions.enabled` (docs-typescript-sdk.md
//! §Sandbox options; `SandboxOptions` in options.d.ts). Enabled denies
//! outbound network by default and limits writes to cwd/temp/sandbox.json
//! allows — pheobe-strict-shaped. Moderate and strict therefore share
//! `enabled`; free is `disabled` plus pheobe's own policy invariants.
//!
//! Env: `PHEOBE_CURSOR_BIN` (default `cursor-agent`),
//! `PHEOBE_CURSOR_FLAGS`, `PHEOBE_CURSOR_TIMEOUT_SECS` (default 3600),
//! `PHEOBE_SANDBOX`. The child inherits `CURSOR_API_KEY` /
//! `CURSOR_API_ENDPOINT`.

use crate::worker::{Worker, WorkerOutcome};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::time::Duration;

const DEFAULT_BIN: &str = "cursor-agent";
const DEFAULT_FLAGS: &str = "--yolo --trust";
const DEFAULT_TIMEOUT_SECS: u64 = 3600;

/// Registry constructor for `worker::REGISTRY`.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(CursorWorker))
}

/// The cursor adapter. Mockable at the `Worker` seam like any other.
pub struct CursorWorker;

/// `PHEOBE_SANDBOX` → `cursor-agent --sandbox` (CLI choices: enabled|disabled).
pub(crate) fn sandbox_flag(tier: &str) -> Result<&'static str> {
    match tier.trim().to_ascii_lowercase().as_str() {
        "strict" | "moderate" => Ok("enabled"),
        "free" => Ok("disabled"),
        other => bail!(
            "unknown PHEOBE_SANDBOX tier '{other}' — cursor adapter expects \
             strict | moderate | free (defaults to moderate when unset)"
        ),
    }
}

impl Worker for CursorWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_CURSOR_BIN").unwrap_or_else(|_| DEFAULT_BIN.to_string());
        let flags =
            std::env::var("PHEOBE_CURSOR_FLAGS").unwrap_or_else(|_| DEFAULT_FLAGS.to_string());
        let timeout = std::env::var("PHEOBE_CURSOR_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        let tier = std::env::var("PHEOBE_SANDBOX").unwrap_or_else(|_| "moderate".into());
        let sandbox = sandbox_flag(&tier)?;

        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json")
            .arg("--sandbox")
            .arg(sandbox);
        cmd.args(flags.split_whitespace());
        cmd.arg(prompt);
        cmd.current_dir(worktree);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().with_context(|| {
            format!(
                "cursor worker: failed to spawn '{bin}' (is the cursor-agent binary on PATH? \
                 set PHEOBE_CURSOR_BIN to override)"
            )
        })?;

        let mut out_pipe = child
            .stdout
            .take()
            .context("cursor worker: no stdout pipe")?;
        let mut err_pipe = child
            .stderr
            .take()
            .context("cursor worker: no stderr pipe")?;
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
            .with_context(|| format!("cursor worker: waiting on '{bin}' failed"))?
            .with_context(|| {
                format!(
                    "cursor worker: '{bin}' timed out after {timeout}s and was killed \
                     (set PHEOBE_CURSOR_TIMEOUT_SECS to adjust; the aging ladder in \
                     run_worker judges the run separately)"
                )
            })?;

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        if !status.success() {
            bail!(
                "cursor worker: '{bin}' exited with {status}: {}",
                stderr_tail(&stderr)
            );
        }

        Ok(parse_stdout(&stdout))
    }
}

/// Parse cursor-agent's stdout defensively. With `--output-format json` the
/// shape is one result object per line or a single object; an unparseable
/// tail degrades to "the whole stdout is the engine's prose" (the claude
/// adapter's defensive posture, reused verbatim).
pub(crate) fn parse_stdout(raw: &str) -> WorkerOutcome {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            return from_result_object(&v);
        }
        let mut last: Option<Value> = None;
        for line in trimmed.lines() {
            let line = line.trim();
            if !line.starts_with('{') {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                last = Some(v);
            }
        }
        if let Some(v) = last {
            return from_result_object(&v);
        }
    }
    WorkerOutcome {
        final_text: raw.to_string(),
        tokens: None,
        usd: None,
        json_tail: None,
    }
}

/// Extract a `WorkerOutcome` from one cursor result JSON object.
///
/// Fields differ across cursor-agent builds and the SDK (`TokenUsage` /
/// `UsageCost` in usage-types.d.ts), so this resolves the first present
/// candidate for each slot:
/// - final text: `content`/`result`/`text`/`answer`/`output`
/// - tokens: `usage.tokens` | `usage.total_tokens` | `usage.totalTokens`
/// - usd: `usage.cost.chargedCents` (or `charged_cents`) / 100
fn from_result_object(v: &Value) -> WorkerOutcome {
    let final_text = ["content", "result", "text", "answer", "output"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| v.to_string());
    let json_tail = crate::worker::extract_json_tail(&final_text);
    WorkerOutcome {
        final_text,
        tokens: tokens_from(v),
        usd: usd_from(v),
        json_tail,
    }
}

fn tokens_from(v: &Value) -> Option<u64> {
    let u = v.get("usage")?;
    u.get("tokens")
        .and_then(json_u64)
        .or_else(|| u.get("total_tokens").and_then(json_u64))
        .or_else(|| u.get("totalTokens").and_then(json_u64))
}

fn usd_from(v: &Value) -> Option<f64> {
    let cost = v
        .get("usage")
        .and_then(|u| u.get("cost"))
        .or_else(|| v.get("cost"))?;
    let cents = cost
        .get("chargedCents")
        .or_else(|| cost.get("charged_cents"))
        .and_then(json_f64)?;
    Some(cents / 100.0)
}

fn json_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| v.as_f64().and_then(|n| (n >= 0.0).then_some(n as u64)))
}

fn json_f64(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_u64().map(|n| n as f64))
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
