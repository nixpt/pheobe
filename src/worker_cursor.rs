//! The cursor worker adapter (PHEOBE-6): `PHEOBE_PROVIDER=cursor`.
//!
//! CLI path (decision recorded in the ticket): the adapter spawns the
//! `cursor-agent` CLI non-interactively — `-p --output-format json` — and
//! parses its stdout defensively, same posture as the opencode/claude/codex
//! adapters: one prompt out, one whole run back, no Node/TS SDK shim. The
//! `@cursor/sdk` route (`Agent.prompt()` with `getUsage()` real-USD budget
//! and `run.steer()` for the aging ladder's mid-run steering) is the
//! recorded follow-up for PHEOBE-16+ steering requirements.
//!
//! argv (verified against the real CLI via `cursor-agent --help` on this
//! box): `cursor-agent -p --output-format json [flags] <prompt>` — cwd =
//! the worktree. Flags come from `PHEOBE_CURSOR_FLAGS`, default
//! `--yolo --trust` (mayfly's proven defaults: `--yolo` = run everything
//! [no per-call approval prompts — the headless posture], `--trust` =
//! trust the workspace without prompting). Setting `PHEOBE_CURSOR_FLAGS`
//! REPLACES the default entirely (same replace-not-append rule as the
//! claude adapter).
//!
//! Sandbox note: cursor's sandbox is a CLI/server-side concept
//! (`--sandbox <mode>`); at the CLI level the flags ARE the tier — the
//! cursor adapter runs `free`-equivalent posture and pheobe's own policy
//! invariants still hold around the run (paths_allow at commit, no
//! protected branches, done_when gate). Mapping `PHEOBE_SANDBOX` onto
//! `--sandbox` is left for the SDK follow-up where sandboxOptions is
//! first-class.
//!
//! Env: `PHEOBE_CURSOR_BIN` (default `cursor-agent`),
//! `PHEOBE_CURSOR_FLAGS`, `PHEOBE_CURSOR_TIMEOUT_SECS` (default 3600).
//! The child inherits the environment (CURSOR_API_KEY / CURSOR_API_ENDPOINT
//! come from the caller).

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

impl Worker for CursorWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_CURSOR_BIN").unwrap_or_else(|_| DEFAULT_BIN.to_string());
        let flags = std::env::var("PHEOBE_CURSOR_FLAGS").unwrap_or_else(|_| DEFAULT_FLAGS.to_string());
        let timeout = std::env::var("PHEOBE_CURSOR_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);

        let mut cmd = std::process::Command::new(&bin);
        cmd.arg("-p")
            .arg("--output-format")
            .arg("json");
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

        let mut out_pipe = child.stdout.take().context("cursor worker: no stdout pipe")?;
        let mut err_pipe = child.stderr.take().context("cursor worker: no stderr pipe")?;
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
fn parse_stdout(raw: &str) -> WorkerOutcome {
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
    WorkerOutcome { final_text: raw.to_string(), tokens: None, usd: None, json_tail: None }
}

/// Extract a `WorkerOutcome` from one cursor result JSON object. The fields
/// differ across cursor-agent build variants, so this resolves the first
/// present candidate for each slot:
/// - final text: `content`/`result`/`text`/`answer`/`output`
/// - tokens: `usage.tokens` or a `total_tokens` snippet
fn from_result_object(v: &Value) -> WorkerOutcome {
    let final_text = ["content", "result", "text", "answer", "output"]
        .iter()
        .find_map(|k| v.get(*k).and_then(|x| x.as_str()))
        .map(|s| s.to_string())
        .unwrap_or_else(|| v.to_string());
    let tokens = v
        .get("usage")
        .and_then(|u| u.get("tokens").and_then(|t| t.as_u64()))
        .or_else(|| {
            v.get("usage")
                .and_then(|u| u.get("total_tokens").and_then(|t| t.as_u64()))
        });
    let json_tail =
        serde_json::from_str::<Value>(&final_text).ok().filter(|j| j.is_object());
    WorkerOutcome { final_text, tokens, usd: None, json_tail }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pheobe-cursor-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_cursor(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
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

    fn capture_cursor(dir: &std::path::Path, final_text: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let capture = dir.join("captured.txt");
        let json = serde_json::json!({
            "content": final_text,
            "usage": { "tokens": 77 }
        });
        let body = format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'\ndone\nprintf '%s\\n' \"CWD=$(pwd)\" >> '{}'\necho '{}'\n",
            capture.display(),
            capture.display(),
            json
        );
        let script = fake_cursor(dir, "cursor-agent", &body);
        (script, capture)
    }

    #[test]
    fn cursor_argv_shape_cwd_and_prompt_reach_the_binary() {
        let dir = scratch("argv");
        let (script, capture) = capture_cursor(&dir, "ok");
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", &script) };
        let w = CursorWorker;
        let out = w.run("do the thing", &dir).unwrap();
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        assert_eq!(out.final_text, "ok");
        let captured = std::fs::read_to_string(&capture).unwrap();
        let lines: Vec<&str> = captured.lines().collect();
        assert_eq!(lines[0], "-p");
        assert_eq!(lines[1], "--output-format");
        assert_eq!(lines[2], "json");
        assert_eq!(lines[3], "--yolo", "default flags apply");
        assert_eq!(lines[4], "--trust");
        assert_eq!(lines[5], "do the thing", "prompt is one argv element");
        assert_eq!(lines[lines.len() - 1], &format!("CWD={}", dir.display()), "cwd = worktree");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cursor_flags_default_is_replaced_not_appended() {
        let dir = scratch("flags");
        let (script, capture) = capture_cursor(&dir, "ok");
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", &script) };
        unsafe { std::env::set_var("PHEOBE_CURSOR_FLAGS", "ccf-mode") };
        let w = CursorWorker;
        w.run("x", &dir).unwrap();
        unsafe { std::env::remove_var("PHEOBE_CURSOR_FLAGS") };
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        let captured = std::fs::read_to_string(&capture).unwrap();
        let lines: Vec<&str> = captured.lines().collect();
        assert_eq!(lines[3], "ccf-mode", "replaces the default entirely");
        assert!(!captured.contains("--yolo"), "default gone when overridden");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cursor_tokens_extracted_from_the_result_object() {
        let dir = scratch("tokens");
        let script = fake_cursor(
            dir.as_path(),
            "cursor-agent",
            r#"echo '{"content":"done","usage":{"total_tokens":42}}'"#,
        );
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", &script) };
        let out = CursorWorker.run("x", &dir).unwrap();
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        assert_eq!(out.final_text, "done");
        assert_eq!(out.tokens, Some(42), "usage.total_tokens resolves");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cursor_stream_of_objects_last_wins() {
        let dir = scratch("stream");
        let script = fake_cursor(
            dir.as_path(),
            "cursor-agent",
            "echo '{\"content\":\"first\"}'; echo '{\"content\":\"second\",\"usage\":{\"tokens\":9}}'",
        );
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", &script) };
        let out = CursorWorker.run("x", &dir).unwrap();
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        assert_eq!(out.final_text, "second", "last object wins");
        assert_eq!(out.tokens, Some(9));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cursor_unparseable_stdout_degrades_to_prose() {
        let dir = scratch("prose");
        let script = fake_cursor(
            dir.as_path(),
            "cursor-agent",
            "echo 'the fix landed — parser now handles nested blocks'",
        );
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", &script) };
        let out = CursorWorker.run("x", &dir).unwrap();
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        assert!(out.final_text.contains("the fix landed"), "prose fallback");
        assert_eq!(out.tokens, None);
        assert_eq!(out.json_tail, None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cursor_json_report_contract_becomes_the_json_tail() {
        let dir = scratch("tail");
        let script = fake_cursor(
            dir.as_path(),
            "cursor-agent",
            r#"echo '{"content":"{\"ok\":true,\"summary\":\"landed\"}"}'"#,
        );
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", &script) };
        let out = CursorWorker.run("x", &dir).unwrap();
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        assert_eq!(out.final_text, "{\"ok\":true,\"summary\":\"landed\"}");
        let tail = out.json_tail.expect("report-contract object becomes the tail");
        assert_eq!(tail["ok"], true);
        assert_eq!(tail["summary"], "landed");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cursor_missing_binary_errors_clearly() {
        let dir = scratch("missing");
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", "/nonexistent/pheobe-fake-cursor") };
        let err = format!("{:#}", CursorWorker.run("x", &dir).unwrap_err());
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        assert!(err.contains("failed to spawn '/nonexistent/pheobe-fake-cursor'"), "got: {err}");
        assert!(err.contains("PHEOBE_CURSOR_BIN"), "names the env override, got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cursor_failed_run_surfaces_the_stderr_tail() {
        let dir = scratch("failed");
        let script = fake_cursor(
            dir.as_path(),
            "cursor-agent",
            "echo 'nope: cursor auth expired' >&2; exit 1",
        );
        unsafe { std::env::set_var("PHEOBE_CURSOR_BIN", &script) };
        let err = format!("{:#}", CursorWorker.run("x", &dir).unwrap_err());
        unsafe { std::env::remove_var("PHEOBE_CURSOR_BIN") };
        assert!(err.contains("exited with"), "got: {err}");
        assert!(err.contains("cursor auth expired"), "stderr tail included, got: {err}");
        std::fs::remove_dir_all(&dir).ok();
    }
}