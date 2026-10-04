//! The cline worker adapter (PHEOBE-49): `PHEOBE_PROVIDER=cline`.
//!
//! One prompt out, one whole cline run back:
//! `cline <prompt> --json --auto-approve true -c <worktree> [-t secs]
//! [-P provider] [-m model] [--data-dir dir] <flags>` — cline 3.x's headless
//! act mode (verified against `cline --help`, 3.0.62). The worktree is both the
//! child's cwd and `-c`.
//!
//! Event stream (verified live, 3.0.62): newline-delimited JSON objects with a
//! `type` — `hook_event`, `agent_event`, `error`, and one final
//! `{"type":"run_result","finishReason":..,"iterations":..,"usage":{..},
//! "aggregateUsage":{..},"text":..,"model":{..}}`. The adapter reads that last
//! `run_result`: `text` is the engine's prose, `aggregateUsage` (else
//! `usage`) gives tokens + `totalCost`, `iterations` gives turns, and
//! `finishReason: "error"` is a hard error carrying `text` (e.g. a provider
//! quota message). Report normalization is the same as every other worker:
//! only the handoff keys of a JSON-shaped final message merge (`json_tail`).
//!
//! Env:
//! - `PHEOBE_CLINE_BIN` (default `cline`)
//! - `PHEOBE_CLINE_PROVIDER` — `-P <id>` (cline's own default otherwise)
//! - `PHEOBE_CLINE_MODEL` — `-m <id>`; wins over the task's `model`
//! - `PHEOBE_CLINE_DATA_DIR` — `--data-dir <dir>` (cline's isolated state)
//! - `PHEOBE_CLINE_FLAGS` — extra flags, split on whitespace, appended last
//! - `PHEOBE_CLINE_TIMEOUT_SECS` (default 3600) — min(this, task ttl) bounds
//!   both our wait and cline's own `-t`.
//!
//! Sandbox: like the other CLI engines — `strict` is refused (cline calls a
//! hosted model), `moderate` runs it under bwrap with `~/.cline` writable
//! (auth refresh, sessions, settings), `free` is a plain subprocess.

use crate::worker::{Worker, WorkerCtx, WorkerOutcome};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

const DEFAULT_TIMEOUT_SECS: u64 = 3600;

/// cline's state under `$HOME` that must stay writable in the moderate tier.
pub(crate) const CLINE_HOME_RW: &[&str] = &[".cline"];

/// Registry constructor for `worker::REGISTRY`.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(ClineWorker))
}

/// The cline adapter. Mockable at the `Worker` seam like any other.
pub struct ClineWorker;

/// Optional cline knobs resolved from the env.
#[derive(Debug, Default, Clone)]
pub(crate) struct ClineOpts {
    pub provider: Option<String>,
    pub model: Option<String>,
    pub data_dir: Option<String>,
    pub flags: String,
    pub timeout_secs: u64,
}

impl Worker for ClineWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        self.run_with(prompt, worktree, &WorkerCtx::default())
    }

    fn run_with(&self, prompt: &str, worktree: &Path, ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_CLINE_BIN").unwrap_or_else(|_| "cline".to_string());
        let env_timeout =
            crate::engine::env_secs("PHEOBE_CLINE_TIMEOUT_SECS", DEFAULT_TIMEOUT_SECS);
        let timeout = crate::engine::effective_timeout(env_timeout, ctx.ttl);
        let nonempty = |v: String| {
            let v = v.trim().to_string();
            (!v.is_empty()).then_some(v)
        };
        let opts = ClineOpts {
            provider: std::env::var("PHEOBE_CLINE_PROVIDER")
                .ok()
                .and_then(nonempty),
            model: crate::engine::resolve_model(
                std::env::var("PHEOBE_CLINE_MODEL").ok(),
                ctx.model.as_deref(),
            ),
            data_dir: std::env::var("PHEOBE_CLINE_DATA_DIR")
                .ok()
                .and_then(nonempty),
            flags: std::env::var("PHEOBE_CLINE_FLAGS").unwrap_or_default(),
            timeout_secs: timeout,
        };
        let argv = cline_args(prompt, worktree, &opts);

        let mut cmd = crate::engine::command(
            "cline",
            &bin,
            &argv,
            worktree,
            ctx.sandbox.as_ref(),
            CLINE_HOME_RW,
            &[],
        )?;
        cmd.current_dir(worktree);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = crate::worker::spawn_retry(&mut cmd).with_context(|| {
            format!(
                "cline worker: failed to spawn '{bin}' (is the cline binary on PATH? \
                 set PHEOBE_CLINE_BIN to override)"
            )
        })?;
        let mut out_pipe = child
            .stdout
            .take()
            .context("cline worker: no stdout pipe")?;
        let mut err_pipe = child
            .stderr
            .take()
            .context("cline worker: no stderr pipe")?;
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
            "cline",
            &bin,
            "PHEOBE_CLINE_TIMEOUT_SECS",
        )?;
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        let (outcome, error) = parse_stdout(&stdout);
        // cline exits non-zero on a failed run; its run_result `text` is the
        // useful message (e.g. a provider quota), so prefer it over stderr.
        if let Some(why) = error {
            anyhow::bail!("cline worker: the run failed: {}", tail(&why));
        }
        if !status.success() {
            let why = if stderr.trim().is_empty() {
                "(no stderr)".to_string()
            } else {
                tail(stderr.trim())
            };
            anyhow::bail!("cline worker: '{bin}' exited with {status}: {why}");
        }
        Ok(outcome)
    }
}

/// `<prompt> --json --auto-approve true -c <wt> [-t s] [-P p] [-m m]
/// [--data-dir d] <flags…>`. Extra flags come last so they can override.
pub(crate) fn cline_args(prompt: &str, worktree: &Path, o: &ClineOpts) -> Vec<String> {
    let mut a = vec![
        prompt.to_string(),
        "--json".into(),
        "--auto-approve".into(),
        "true".into(),
        "-c".into(),
        worktree.display().to_string(),
    ];
    if o.timeout_secs > 0 {
        a.push("-t".into());
        a.push(o.timeout_secs.to_string());
    }
    for (flag, v) in [
        ("-P", &o.provider),
        ("-m", &o.model),
        ("--data-dir", &o.data_dir),
    ] {
        if let Some(v) = v {
            a.push(flag.into());
            a.push(v.clone());
        }
    }
    a.extend(o.flags.split_whitespace().map(String::from));
    a
}

/// Parse cline's `--json` stream: the last `run_result` wins. Returns the
/// outcome and, when the run failed, the reason. No `run_result` degrades to
/// "the whole stdout is the engine's prose" (never a silent success on an
/// `error` event, though).
pub(crate) fn parse_stdout(raw: &str) -> (WorkerOutcome, Option<String>) {
    let mut result: Option<Value> = None;
    let mut last_error: Option<String> = None;
    for line in raw.lines().map(str::trim).filter(|l| l.starts_with('{')) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match v.get("type").and_then(Value::as_str) {
            Some("run_result") => result = Some(v),
            Some("error") => {
                last_error = v.get("message").and_then(Value::as_str).map(String::from)
            }
            _ => {}
        }
    }
    let Some(r) = result else {
        let outcome = WorkerOutcome {
            final_text: raw.to_string(),
            tokens: None,
            usd: None,
            json_tail: None,
            turns: None,
        };
        return (outcome, last_error);
    };
    let text = r
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let usage = r.get("aggregateUsage").or_else(|| r.get("usage"));
    let tokens = usage.and_then(sum_usage);
    let usd = usage
        .and_then(|u| u.get("totalCost"))
        .and_then(Value::as_f64);
    let turns = r
        .get("iterations")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok());
    let failed = r.get("finishReason").and_then(Value::as_str) == Some("error");
    let error = failed.then(|| {
        if text.trim().is_empty() {
            last_error
                .clone()
                .unwrap_or_else(|| "finishReason=error".into())
        } else {
            text.clone()
        }
    });
    let json_tail = crate::worker::extract_json_tail(&text);
    let outcome = WorkerOutcome {
        final_text: text,
        tokens,
        usd,
        json_tail,
        turns,
    };
    (outcome, error)
}

/// input + output + both cache classes, as the budget estimator counts.
fn sum_usage(u: &Value) -> Option<u64> {
    let mut total = 0u64;
    let mut any = false;
    for k in [
        "inputTokens",
        "outputTokens",
        "cacheReadTokens",
        "cacheWriteTokens",
    ] {
        if let Some(n) = u.get(k).and_then(Value::as_u64) {
            total += n;
            any = true;
        }
    }
    any.then_some(total)
}

fn tail(text: &str) -> String {
    const MAX: usize = 400;
    if text.chars().count() <= MAX {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(MAX).collect::<String>())
    }
}
