//! The GitHub Copilot CLI worker adapter (PHEOBE-56): `PHEOBE_PROVIDER=copilot`.
//!
//! One prompt out, one whole Copilot run back:
//! `copilot -p <prompt> --output-format json --no-ask-user -C <worktree>
//! --deny-tool=shell(git push) --deny-tool=shell(gh) [--model m]
//! --usage-output-file <wt>/.pheobe/copilot-usage.json <flags>`
//! (verified against `copilot --help`, Copilot CLI 1.0.98). The worktree is
//! both the child's cwd and `-C`, so Copilot's own path verification (cwd +
//! subdirs + temp) is scoped to pheobe's kitchen.
//!
//! Flags come from `PHEOBE_COPILOT_FLAGS`, default
//! `--allow-all-tools --disable-builtin-mcps`. In `-p` mode a tool that needs
//! approval is denied, never prompted, so a headless engine needs
//! `--allow-all-tools` to edit files and run `done_when`. The built-in GitHub
//! MCP server is off: pheobe's engine works in a worktree and never talks to
//! GitHub (pheobe never pushes). Setting `PHEOBE_COPILOT_FLAGS` REPLACES the
//! default entirely (cursor/agy convention); the mechanical args above are
//! always passed.
//!
//! The two `--deny-tool` rules are pheobe policy, not flags, so no
//! `PHEOBE_COPILOT_FLAGS` can drop them: the parent merges, pheobe never pushes
//! or opens PRs, and the engine inherits a GitHub token (it is Copilot's auth).
//! A Copilot deny rule beats every allow rule, `--allow-all-tools` included.
//! `shell(gh)` with no subcommand matches every `gh` invocation.
//!
//! Event stream (verified live, 1.0.98): `--output-format json` is JSONL, one
//! `{"type":..,"data":{..}}` object per line — `assistant.message`
//! (`data.content`, empty on a tool-call-only message), `assistant.turn_end`,
//! `tool.execution_*`, … and one final `{"type":"result","exitCode":..,
//! "usage":{"premiumRequests":..,"codeChanges":{..}}}`. The stream carries no
//! token counts, so the adapter also asks for `--usage-output-file`, whose
//! `tokenDetails.{input,cache_read,cache_write,output}.tokenCount` give tokens.
//! The file lives under the worktree's `.pheobe/` (git-excluded, PHEOBE-48) so
//! it survives the moderate tier's private `/tmp`; the adapter deletes it after
//! reading.
//!
//! - final text: the last non-empty `assistant.message` content
//! - turns: the count of `assistant.turn_end` events
//! - usd: none — Copilot bills premium requests / AI credits, not USD, and no
//!   price table is invented (codex precedent)
//! - failure: a non-zero `result.exitCode` (or process exit) is a hard error
//!   carrying the last `*error*` event's message
//!
//! Env:
//! - `PHEOBE_COPILOT_BIN` (default `copilot`)
//! - `PHEOBE_COPILOT_MODEL` — `--model <id>`; wins over the task's `model`
//! - `PHEOBE_COPILOT_FLAGS` (default `--allow-all-tools --disable-builtin-mcps`)
//! - `PHEOBE_COPILOT_TIMEOUT_SECS` (default 3600) — min(this, task ttl)
//!
//! Auth is Copilot's own: `COPILOT_GITHUB_TOKEN` / `GH_TOKEN` / `GITHUB_TOKEN`
//! (inherited) or a stored `copilot login` under `~/.copilot` (`COPILOT_HOME`).
//!
//! Sandbox: `strict` is refused (Copilot calls a hosted model), `moderate`
//! runs the whole engine under pheobe's bwrap with `~/.copilot` (or
//! `$COPILOT_HOME`) writable, `free` is a plain subprocess. Copilot's own
//! `--sandbox` (MXC) only wraps shell commands and needs slirp4netns +
//! iptables, so pheobe's bwrap is the tier, as for claude/opencode.

use crate::worker::{Worker, WorkerCtx, WorkerOutcome};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Stdio;

const DEFAULT_BIN: &str = "copilot";
const DEFAULT_FLAGS: &str = "--allow-all-tools --disable-builtin-mcps";
const DEFAULT_TIMEOUT_SECS: u64 = 3600;
/// Always denied (see the module docs): pheobe never pushes or opens PRs.
const DENY_TOOLS: &[&str] = &["shell(git push)", "shell(gh)"];

/// Copilot's state under `$HOME` that must stay writable in the moderate tier
/// (stored login, session store, logs, auto-updated package cache).
pub(crate) const COPILOT_HOME_RW: &[&str] = &[".copilot"];
/// `COPILOT_HOME` relocates that state dir (`copilot help environment`).
pub(crate) const COPILOT_ENV_RW: &[&str] = &["COPILOT_HOME"];

/// Registry constructor for `worker::REGISTRY`.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(CopilotWorker))
}

/// The copilot adapter. Mockable at the `Worker` seam like any other.
pub struct CopilotWorker;

/// Where the adapter asks Copilot to write its usage summary.
pub(crate) fn usage_file(worktree: &Path) -> PathBuf {
    worktree.join(".pheobe").join("copilot-usage.json")
}

impl Worker for CopilotWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        self.run_with(prompt, worktree, &WorkerCtx::default())
    }

    fn run_with(&self, prompt: &str, worktree: &Path, ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_COPILOT_BIN").unwrap_or_else(|_| DEFAULT_BIN.to_string());
        let flags =
            std::env::var("PHEOBE_COPILOT_FLAGS").unwrap_or_else(|_| DEFAULT_FLAGS.to_string());
        let timeout = crate::engine::effective_timeout(
            crate::engine::env_secs("PHEOBE_COPILOT_TIMEOUT_SECS", DEFAULT_TIMEOUT_SECS),
            ctx.ttl,
        );
        let model = crate::engine::resolve_model(
            std::env::var("PHEOBE_COPILOT_MODEL").ok(),
            ctx.model.as_deref(),
        );
        let usage = usage_file(worktree);
        let usage_arg = match usage.parent().map(std::fs::create_dir_all) {
            Some(Ok(())) => {
                let _ = std::fs::remove_file(&usage); // never read a stale summary
                Some(usage.as_path())
            }
            _ => None, // no sidecar dir: run without token counts rather than fail
        };
        let argv = copilot_args(prompt, worktree, model.as_deref(), usage_arg, &flags);

        let mut cmd = crate::engine::command(
            "copilot",
            &bin,
            &argv,
            worktree,
            ctx.sandbox.as_ref(),
            COPILOT_HOME_RW,
            COPILOT_ENV_RW,
        )?;
        cmd.current_dir(worktree);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = crate::worker::spawn_retry(&mut cmd).with_context(|| {
            format!(
                "copilot worker: failed to spawn '{bin}' (is the GitHub Copilot CLI on PATH? \
                 `npm i -g @github/copilot`, or set PHEOBE_COPILOT_BIN)"
            )
        })?;
        let mut out_pipe = child
            .stdout
            .take()
            .context("copilot worker: no stdout pipe")?;
        let mut err_pipe = child
            .stderr
            .take()
            .context("copilot worker: no stderr pipe")?;
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
            "copilot",
            &bin,
            "PHEOBE_COPILOT_TIMEOUT_SECS",
        )?;
        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        let (mut outcome, error) = parse_stdout(&stdout);
        if usage_arg.is_some() {
            if let Ok(raw) = std::fs::read_to_string(&usage) {
                outcome.tokens = serde_json::from_str::<Value>(&raw)
                    .ok()
                    .as_ref()
                    .and_then(tokens_from_usage);
            }
            let _ = std::fs::remove_file(&usage);
        }
        if let Some(why) = error {
            anyhow::bail!("copilot worker: the run failed: {}", tail(&why));
        }
        if !status.success() {
            let why = if stderr.trim().is_empty() {
                if stdout.trim().is_empty() {
                    "(no output)".to_string()
                } else {
                    tail(stdout.trim())
                }
            } else {
                tail(stderr.trim())
            };
            anyhow::bail!("copilot worker: '{bin}' exited with {status}: {why}");
        }
        Ok(outcome)
    }
}

/// `-p <prompt> --output-format json --no-ask-user -C <wt> --deny-tool=…
/// [--model m] [--usage-output-file f] <flags…>`. `--deny-tool` takes the
/// `=` form because it is variadic: a bare value after it would swallow the
/// next positional. Flags come last so they can add to
/// (or, for repeatable options, extend) the fixed shape.
pub(crate) fn copilot_args(
    prompt: &str,
    worktree: &Path,
    model: Option<&str>,
    usage_file: Option<&Path>,
    flags: &str,
) -> Vec<String> {
    let mut a = vec![
        "-p".into(),
        prompt.to_string(),
        "--output-format".into(),
        "json".into(),
        "--no-ask-user".into(),
        "-C".into(),
        worktree.display().to_string(),
    ];
    a.extend(DENY_TOOLS.iter().map(|t| format!("--deny-tool={t}")));
    if let Some(m) = model {
        a.push("--model".into());
        a.push(m.to_string());
    }
    if let Some(f) = usage_file {
        a.push("--usage-output-file".into());
        a.push(f.display().to_string());
    }
    a.extend(flags.split_whitespace().map(String::from));
    a
}

/// Parse Copilot's JSONL stream. Returns the outcome and, when the run failed,
/// the reason. No JSON at all degrades to "the whole stdout is the engine's
/// prose" (the claude/cursor defensive posture).
pub(crate) fn parse_stdout(raw: &str) -> (WorkerOutcome, Option<String>) {
    let mut text: Option<String> = None;
    let mut turns: u32 = 0;
    let mut saw_event = false;
    let mut exit_code: Option<i64> = None;
    let mut last_error: Option<String> = None;
    for line in raw.lines().map(str::trim).filter(|l| l.starts_with('{')) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(kind) = v.get("type").and_then(Value::as_str) else {
            continue;
        };
        saw_event = true;
        let data = v.get("data");
        match kind {
            "assistant.message" => {
                let content = data
                    .and_then(|d| d.get("content"))
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                if !content.trim().is_empty() {
                    text = Some(content.to_string());
                }
            }
            "assistant.turn_end" => turns += 1,
            "result" => exit_code = v.get("exitCode").and_then(Value::as_i64),
            k if k.contains("error") => {
                last_error = data
                    .and_then(|d| d.get("message").or_else(|| d.get("error")))
                    .and_then(Value::as_str)
                    .map(String::from)
                    .or_else(|| Some(k.to_string()));
            }
            _ => {}
        }
    }
    if !saw_event {
        let outcome = WorkerOutcome {
            final_text: raw.to_string(),
            tokens: None,
            usd: None,
            json_tail: None,
            turns: None,
        };
        return (outcome, None);
    }
    let error = match exit_code {
        Some(0) => None,
        Some(code) => Some(
            last_error
                .clone()
                .unwrap_or_else(|| format!("result exitCode={code}")),
        ),
        // stream ended without a `result`: an error event is a failure
        None => last_error.clone(),
    };
    let final_text = text.unwrap_or_default();
    let json_tail = crate::worker::extract_json_tail(&final_text);
    let outcome = WorkerOutcome {
        final_text,
        tokens: None,
        usd: None,
        json_tail,
        turns: (turns > 0).then_some(turns),
    };
    (outcome, error)
}

/// Tokens from `--usage-output-file`: input + cache read + cache write +
/// output (`tokenDetails`), else the per-model `usage.inputTokens +
/// outputTokens` (where `inputTokens` already includes the cache classes).
pub(crate) fn tokens_from_usage(u: &Value) -> Option<u64> {
    if let Some(d) = u.get("tokenDetails") {
        let mut total = 0u64;
        let mut any = false;
        for k in ["input", "cache_read", "cache_write", "output"] {
            if let Some(n) = d
                .get(k)
                .and_then(|x| x.get("tokenCount"))
                .and_then(Value::as_u64)
            {
                total += n;
                any = true;
            }
        }
        if any {
            return Some(total);
        }
    }
    let models = u.get("modelMetrics")?.as_object()?;
    let mut total = 0u64;
    let mut any = false;
    for m in models.values() {
        for k in ["inputTokens", "outputTokens"] {
            if let Some(n) = m
                .get("usage")
                .and_then(|x| x.get(k))
                .and_then(Value::as_u64)
            {
                total += n;
                any = true;
            }
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
