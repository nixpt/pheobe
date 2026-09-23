//! The AGY worker adapter (PHEOBE-25): `PHEOBE_PROVIDER=agy` (or `antigravity`).
//!
//! CLI path: spawn `agy` non-interactively in print mode:
//! `agy -p <prompt> --output-format json --dangerously-skip-permissions`
//! with `cwd` set to the worktree pheobe provisioned.
//!
//! Flags:
//! - Default: `--dangerously-skip-permissions` (required for headless tool
//!   execution; without it, jetski auto-denies tool calls).
//! - `PHEOBE_AGY_FLAGS` replaces the default flags entirely (claude/cursor convention).
//!
//! Sandbox tier mapping:
//! - `strict` → `--sandbox` (terminal restrictions enabled).
//! - `moderate` (default) → no `--sandbox` flag (pheobe workspace containment).
//! - `free` → no `--sandbox` flag.
//!
//! Env:
//! - `PHEOBE_AGY_BIN` (default `agy`)
//! - `PHEOBE_AGY_FLAGS` (default `--dangerously-skip-permissions`)
//! - `PHEOBE_AGY_TIMEOUT_SECS` (default 3600)
//! - `PHEOBE_SANDBOX` (strict | moderate | free)

use crate::worker::{Worker, WorkerCtx, WorkerOutcome};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;

const DEFAULT_BIN: &str = "agy";
const DEFAULT_FLAGS: &str = "--dangerously-skip-permissions";
const DEFAULT_TIMEOUT_SECS: u64 = 3600;

/// Registry constructor for `worker::REGISTRY`.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(AgyWorker))
}

/// The agy adapter. Mockable at the `Worker` seam.
pub struct AgyWorker;

/// `PHEOBE_SANDBOX` → whether to pass `--sandbox` to `agy`.
pub(crate) fn sandbox_enabled(tier: &str) -> Result<bool> {
    match tier.trim().to_ascii_lowercase().as_str() {
        "strict" => Ok(true),
        "moderate" | "free" => Ok(false),
        other => bail!(
            "unknown PHEOBE_SANDBOX tier '{other}' — agy adapter expects \
             strict | moderate | free (defaults to moderate when unset)"
        ),
    }
}

impl Worker for AgyWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        self.run_with(prompt, worktree, &WorkerCtx::default())
    }

    /// PHEOBE-46: `--model` (`PHEOBE_AGY_MODEL` > task), ttl-bounded timeout,
    /// and the run's tier: strict → agy's native `--sandbox`; moderate →
    /// pheobe's bwrap confinement (agy has none of its own there); free → plain.
    /// A direct `run()` (no resolved tier) keeps the pre-46 env behaviour.
    fn run_with(&self, prompt: &str, worktree: &Path, ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_AGY_BIN").unwrap_or_else(|_| DEFAULT_BIN.to_string());
        let flags = std::env::var("PHEOBE_AGY_FLAGS").unwrap_or_else(|_| DEFAULT_FLAGS.to_string());
        let timeout = crate::engine::effective_timeout(
            crate::engine::env_secs("PHEOBE_AGY_TIMEOUT_SECS", DEFAULT_TIMEOUT_SECS),
            ctx.ttl,
        );
        let tier = crate::engine::tier_or_env(ctx.sandbox.as_ref())?;
        let use_sandbox = sandbox_enabled(tier.as_str())?;
        let model = crate::engine::resolve_model(
            std::env::var("PHEOBE_AGY_MODEL").ok(),
            ctx.model.as_deref(),
        );

        let mut argv: Vec<String> = vec![
            "-p".into(),
            prompt.to_string(),
            "--output-format".into(),
            "json".into(),
        ];
        if use_sandbox {
            argv.push("--sandbox".into());
        }
        if let Some(m) = &model {
            argv.extend(["--model".into(), m.clone()]);
        }
        argv.extend(flags.split_whitespace().map(str::to_string));
        // bwrap only for a RESOLVED moderate tier (run_worker); strict uses
        // agy's own --sandbox above, so pheobe spawns it plain.
        let bwrap_tier = match &ctx.sandbox {
            Some(crate::sandbox::Tier::Moderate) => Some(crate::sandbox::Tier::Moderate),
            _ => None,
        };
        let mut cmd = crate::engine::command(
            "agy",
            &bin,
            &argv,
            worktree,
            bwrap_tier.as_ref(),
            crate::engine::AGY_HOME_RW,
        )?;
        cmd.current_dir(worktree);
        cmd.stdin(std::process::Stdio::null());
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let mut child = crate::worker::spawn_retry(&mut cmd).with_context(|| {
            format!(
                "agy worker: failed to spawn '{bin}' (is the agy binary on PATH? \
                 set PHEOBE_AGY_BIN to override)"
            )
        })?;

        let mut out_pipe = child.stdout.take().context("agy worker: no stdout pipe")?;
        let mut err_pipe = child.stderr.take().context("agy worker: no stderr pipe")?;

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

        let status = crate::engine::wait_or_kill(
            &mut child,
            timeout,
            "agy",
            &bin,
            "PHEOBE_AGY_TIMEOUT_SECS",
        )?;

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        if !status.success() {
            bail!(
                "agy worker: '{bin}' exited with {status}: {}",
                stderr_tail(&stderr, &stdout)
            );
        }

        let (outcome, is_err) = parse_stdout(&stdout);
        if is_err {
            bail!(
                "agy worker: '{bin}' reported failure status: {}",
                text_tail(&outcome.final_text)
            );
        }

        Ok(outcome)
    }
}

/// Parse agy stdout defensively. With `--output-format json` the primary
/// payload is a single JSON object. A preamble (e.g. from jetski permissions
/// warning) before the JSON or a stream of lines is tolerated.
pub(crate) fn parse_stdout(raw: &str) -> (WorkerOutcome, bool) {
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
    (
        WorkerOutcome {
            final_text: raw.to_string(),
            tokens: None,
            usd: None,
            json_tail: None,
            turns: None,
        },
        false,
    )
}

/// Extract a `WorkerOutcome` from an agy result JSON object.
fn from_result_object(v: &Value) -> (WorkerOutcome, bool) {
    let is_err = v
        .get("status")
        .and_then(|s| s.as_str())
        .map(|s| s != "SUCCESS")
        .unwrap_or(false);

    let final_text = ["response", "content", "result", "text", "answer", "output"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| v.to_string());

    let tokens = tokens_from(v);
    let json_tail = crate::worker::extract_json_tail(&final_text);

    (
        WorkerOutcome {
            final_text,
            tokens,
            usd: None,
            json_tail,
            turns: None,
        },
        is_err,
    )
}

fn tokens_from(v: &Value) -> Option<u64> {
    let u = v.get("usage")?;
    if let Some(t) = u.get("total_tokens").and_then(json_u64) {
        return Some(t);
    }
    // Fallback: sum input + output + thinking
    let mut sum = 0u64;
    let mut any = false;
    for key in ["input_tokens", "output_tokens", "thinking_tokens"] {
        if let Some(n) = u.get(key).and_then(json_u64) {
            sum += n;
            any = true;
        }
    }
    any.then_some(sum)
}

fn json_u64(v: &Value) -> Option<u64> {
    v.as_u64()
        .or_else(|| v.as_i64().and_then(|n| u64::try_from(n).ok()))
        .or_else(|| v.as_f64().and_then(|n| (n >= 0.0).then_some(n as u64)))
}

fn stderr_tail(stderr: &str, stdout: &str) -> String {
    let t = stderr.trim();
    if !t.is_empty() {
        text_tail(t)
    } else {
        let o = stdout.trim();
        if !o.is_empty() {
            text_tail(o)
        } else {
            "(no output)".to_string()
        }
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
