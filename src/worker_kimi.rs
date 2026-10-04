//! The kimi worker adapter (PHEOBE-8): `PHEOBE_PROVIDER=kimi`.
//!
//! One prompt out, one whole kimi run back:
//! `kimi -w <worktree> -p <prompt> --print --output-format stream-json <flags>`
//! spawned with cwd = the worktree.
//!
//! The fleet `kimi` binary is the cece-rebranded Kimi Code CLI (the same
//! lineage cece-rs forked from — "cece, your next CLI agent"). Headless
//! approval is automatic: the default flags carry `--afk` (away-from-
//! keyboard: no interactive prompts) and `--yolo` (approve all actions),
//! the same auto-approve policy surface as mayfly's cece adapter.
//!
//! Output: kimi `stream-json` is newline-delimited JSON objects; the
//! adapter takes the last one (its final message) as `final_text` with a
//! prose fallback, and hands the whole tail object up as `json_tail` when
//! it parses as the report contract. Env for the SDK/CLI `Config`: the
//! `KIMI_*` vars (`KIMI_API_KEY`, `KIMI_BASE_URL`, `KIMI_MODEL_NAME`) pick
//! the OpenAI-shaped endpoint — flownet / pipefish `/v1` / any gateway the
//! fleet runs.
//!
//! Env:
//! - `PHEOBE_KIMI_BIN` (default `kimi`)
//! - `PHEOBE_KIMI_FLAGS` — default `--afk --yolo` (split on whitespace).
//!   Setting it REPLACES the default entirely (claude/cursor convention).
//! - `PHEOBE_KIMI_TIMEOUT_SECS` (default 3600) — subprocess timeout.

use crate::worker::{Worker, WorkerCtx, WorkerOutcome};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

const DEFAULT_FLAGS: &str = "--afk --yolo";
const DEFAULT_TIMEOUT_SECS: u64 = 3600;

/// Registry constructor for `worker::REGISTRY`.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(KimiWorker))
}

/// The kimi adapter. Mockable at the `Worker` seam like any other.
pub struct KimiWorker;

impl Worker for KimiWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        self.run_with(prompt, worktree, &WorkerCtx::default())
    }

    /// PHEOBE-46: `-m` (`PHEOBE_KIMI_MODEL` > task), ttl-bounded timeout, and
    /// pheobe's bwrap confinement in `moderate` (kimi has no sandbox of its own).
    fn run_with(&self, prompt: &str, worktree: &Path, ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_KIMI_BIN").unwrap_or_else(|_| "kimi".to_string());
        let flags =
            std::env::var("PHEOBE_KIMI_FLAGS").unwrap_or_else(|_| DEFAULT_FLAGS.to_string());
        let timeout = crate::engine::effective_timeout(
            crate::engine::env_secs("PHEOBE_KIMI_TIMEOUT_SECS", DEFAULT_TIMEOUT_SECS),
            ctx.ttl,
        );
        let model = crate::engine::resolve_model(
            std::env::var("PHEOBE_KIMI_MODEL").ok(),
            ctx.model.as_deref(),
        );
        let mut argv: Vec<String> = vec![
            "-w".into(),
            worktree.display().to_string(),
            "-p".into(),
            prompt.to_string(),
            "--print".into(),
            "--output-format".into(),
            "stream-json".into(),
        ];
        if let Some(m) = &model {
            argv.extend(["-m".into(), m.clone()]);
        }
        argv.extend(flags.split_whitespace().map(str::to_string));
        let mut cmd = crate::engine::command(
            "kimi",
            &bin,
            &argv,
            worktree,
            ctx.sandbox.as_ref(),
            crate::engine::KIMI_HOME_RW,
            crate::engine::KIMI_ENV_RW,
        )?;
        cmd.current_dir(worktree);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = crate::worker::spawn_retry(&mut cmd).with_context(|| {
            format!(
                "kimi worker: failed to spawn '{bin}' (is the kimi binary on PATH? \
                 set PHEOBE_KIMI_BIN to override)"
            )
        })?;

        let mut out_pipe = child.stdout.take().context("kimi worker: no stdout pipe")?;
        let mut err_pipe = child.stderr.take().context("kimi worker: no stderr pipe")?;
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
            "kimi",
            &bin,
            "PHEOBE_KIMI_TIMEOUT_SECS",
        )?;

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        if !status.success() {
            anyhow::bail!(
                "kimi worker: '{bin}' exited with {status}: {}",
                stderr_tail(&stderr)
            );
        }

        // kimi prints errors as `{"error":"..."}` on stdout even with exit 0 —
        // surface those as run failures, not silent prose.
        if let Ok(v) = serde_json::from_str::<Value>(stdout.trim()) {
            if v.get("error").is_some() {
                anyhow::bail!(
                    "kimi worker: the run reported an error: {}",
                    text_tail(v.get("error").and_then(|e| e.as_str()).unwrap_or(""))
                );
            }
        }

        Ok(parse_stdout(&stdout))
    }
}

/// kimi stream-json is one JSON object per line. Take the last one for the
/// final message; if none parse, degrade to the whole stdout as prose.
fn parse_stdout(stdout: &str) -> WorkerOutcome {
    let mut last: Option<Value> = None;
    for line in stdout.lines() {
        if let Ok(v) = serde_json::from_str::<Value>(line) {
            last = Some(v);
        }
    }
    match last {
        // PHEOBE-46: the raw last stream object is NOT the handoff — it is the
        // engine's final wire message. Only a handoff-shaped object extracted
        // from its text may set ok/summary (worker::extract_json_tail), the
        // same filter every other adapter applies.
        Some(v) => {
            let final_text = content_of(&v);
            WorkerOutcome {
                json_tail: crate::worker::extract_json_tail(&final_text),
                final_text,
                tokens: usage_tokens(&v),
                usd: usage_usd(&v),
                turns: None,
            }
        }
        None => WorkerOutcome {
            final_text: stdout.trim().to_string(),
            tokens: None,
            usd: None,
            json_tail: None,
            turns: None,
        },
    }
}

/// The final message text from a kimi wire message: `message.content` parts
/// (strings or content-part objects), falling back to `message` if it's a
/// plain string.
fn content_of(v: &Value) -> String {
    if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
        return text.to_string();
    }
    if let Some(parts) = v.get("content").and_then(|c| c.as_array()) {
        let mut out = String::new();
        for p in parts {
            if let Some(s) = p.as_str() {
                out.push_str(s);
                out.push('\n');
            } else if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
                out.push('\n');
            }
        }
        return out.trim_end().to_string();
    }
    if let Some(s) = v.get("message").and_then(|m| m.as_str()) {
        return s.to_string();
    }
    if let Some(s) = v.get("result").and_then(|r| r.as_str()) {
        return s.to_string();
    }
    String::new()
}

fn usage_tokens(v: &Value) -> Option<u64> {
    v.get("usage")
        .map(|u| {
            u.get("tokens")
                .or_else(|| u.get("total_tokens"))
                .and_then(|t| t.as_u64())
        })
        .unwrap_or_else(|| {
            let mut total = 0u64;
            let mut any = false;
            for key in ["input_tokens", "output_tokens"] {
                if let Some(t) = v
                    .get("usage")
                    .and_then(|u| u.get(key))
                    .and_then(|t| t.as_u64())
                {
                    total += t;
                    any = true;
                }
            }
            any.then_some(total)
        })
}

fn usage_usd(v: &Value) -> Option<f64> {
    v.get("usage")
        .and_then(|u| u.get("usd").or_else(|| u.get("cost")))
        .and_then(|c| c.as_f64())
}

fn stderr_tail(stderr: &str) -> String {
    text_tail(stderr)
}

fn text_tail(s: &str) -> String {
    let mut lines: Vec<&str> = s.lines().collect();
    if lines.len() > 3 {
        lines = lines[lines.len() - 3..].to_vec();
    }
    lines.join(" | ").chars().take(300).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pheobe-kimi-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_kimi(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
        crate::tests::write_shim(dir, name, body)
    }

    fn capture_kimi(dir: &std::path::Path, out: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let capture = dir.join("captured.txt");
        let body = format!(
            "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'\ndone\nprintf '%s\\n' \"CWD=$(pwd)\" >> '{}'\nprintf '%s\\n' '{}'\n",
            capture.display(),
            capture.display(),
            out
        );
        let script = fake_kimi(dir, "kimi", &body);
        (script, capture)
    }

    /// PHEOBE_KIMI_BIN / PHEOBE_KIMI_FLAGS are process-global: parallel kimi
    /// tests raced on them (up to 4 failed when run as a group; pre-existing,
    /// found in PHEOBE-46). Every env-touching kimi test holds this for its body.
    static KIMI_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn kimi_env() -> std::sync::MutexGuard<'static, ()> {
        KIMI_ENV.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn with_kimi_bin<P>(script: &std::path::Path, body: P)
    where
        P: FnOnce(),
    {
        unsafe { std::env::set_var("PHEOBE_KIMI_BIN", script) };
        body();
        unsafe { std::env::remove_var("PHEOBE_KIMI_BIN") };
    }

    #[test]
    fn kimi_argv_shape_worktree_prompt_and_stdout_parse() {
        let _env = kimi_env();
        let dir = scratch("argv");
        let (script, captured) =
            capture_kimi(&dir, r#"{"message":"done ok","usage":{"tokens":55}}"#);
        with_kimi_bin(&script, || {
            let w = KimiWorker;
            let out = w.run("refactor foo", &dir).unwrap();
            assert_eq!(out.final_text, "done ok");
            assert_eq!(out.tokens, Some(55));
            // PHEOBE-46: the raw last wire object is not a handoff — it no longer
            // becomes json_tail (it could fake ok/summary); only handoff-shaped
            // text does (kimi_handoff_text_is_the_json_tail).
            assert!(out.json_tail.is_none(), "{:?}", out.json_tail);
        });
        let args = std::fs::read_to_string(&captured).unwrap();
        assert!(args.contains("-w"));
        assert!(args.contains(&dir.display().to_string()));
        assert!(args.contains("-p"));
        assert!(args.contains("refactor foo"));
        assert!(args.contains("--print"));
        assert!(args.contains("--output-format"));
        assert!(args.contains("stream-json"));
        assert!(args.contains("--afk"));
        assert!(args.contains("--yolo"));
        assert!(args.contains("CWD="));
    }

    #[test]
    fn kimi_handoff_text_is_the_json_tail() {
        // PHEOBE-46: a handoff object in the final message's TEXT is honoured;
        // a non-handoff wire object is not.
        let handoff =
            serde_json::json!({"ok": false, "summary": "tests red", "blocked": "fixture"});
        let line = serde_json::json!({"text": handoff.to_string()}).to_string();
        let out = parse_stdout(&format!("{{\"message\":\"thinking\"}}\n{line}\n"));
        let tail = out
            .json_tail
            .expect("handoff-shaped text must become json_tail");
        assert_eq!(tail["summary"], "tests red");
        assert_eq!(tail["ok"], false);
        let sneaky =
            parse_stdout("{\"ok\":true,\"summary\":\"not a handoff text\",\"usage\":{}}\n");
        // a bare wire object carrying ok/summary (no message text) can no longer
        // speak for the engine: before PHEOBE-46 it became the tail verbatim
        assert!(sneaky.json_tail.is_none(), "{:?}", sneaky.json_tail);
    }

    #[test]
    fn kimi_flags_replace_not_append() {
        let _env = kimi_env();
        let dir = scratch("flags");
        let (script, captured) = capture_kimi(&dir, r#"{"message":"ok"}"#);
        unsafe { std::env::set_var("PHEOBE_KIMI_FLAGS", "--afk") };
        with_kimi_bin(&script, || {
            let out = KimiWorker.run("x", &dir).unwrap();
            assert_eq!(out.final_text, "ok");
        });
        unsafe { std::env::remove_var("PHEOBE_KIMI_FLAGS") };
        let args = std::fs::read_to_string(&captured).unwrap();
        assert!(args.contains("--afk"));
        assert!(
            !args.contains("--yolo"),
            "PHEOBE_KIMI_FLAGS must replace the default"
        );
    }

    #[test]
    fn kimi_stream_objects_last_wins() {
        let _env = kimi_env();
        let dir = scratch("stream");
        let (script, _) = capture_kimi(
            &dir,
            "{\"message\":\"first\",\"usage\":{\"tokens\":1}}\n{\"message\":\"final\",\"usage\":{\"tokens\":2}}",
        );
        with_kimi_bin(&script, || {
            let out = KimiWorker.run("x", &dir).unwrap();
            assert_eq!(out.final_text, "final");
            assert_eq!(out.tokens, Some(2));
        });
    }

    #[test]
    fn kimi_unparseable_stdout_degrades_to_prose() {
        let _env = kimi_env();
        let dir = scratch("prose");
        let (script, _) = capture_kimi(&dir, "just some words, no json");
        with_kimi_bin(&script, || {
            let out = KimiWorker.run("x", &dir).unwrap();
            assert_eq!(out.final_text, "just some words, no json");
            assert!(out.json_tail.is_none());
        });
    }

    #[test]
    fn kimi_error_payload_surfaces_as_failure() {
        let _env = kimi_env();
        let dir = scratch("err");
        let (script, _) = capture_kimi(&dir, "{\"error\":\"Insufficient Balance\"}");
        with_kimi_bin(&script, || {
            let w = KimiWorker;
            let err = w.run("x", &dir).unwrap_err();
            assert!(err.to_string().contains("Insufficient Balance"));
        });
    }

    #[test]
    fn kimi_content_parts_concat_as_final_text() {
        let _env = kimi_env();
        let dir = scratch("parts");
        let body =
            "printf '%s\\n' '{\"content\":[{\"text\":\"part one\"},{\"text\":\"part two\"}]}'"
                .to_string();
        let script = fake_kimi(&dir, "kimi", &body);
        with_kimi_bin(&script, || {
            let out = KimiWorker.run("x", &dir).unwrap();
            assert_eq!(out.final_text, "part one\npart two");
        });
    }

    #[test]
    fn kimi_missing_binary_errors_clearly() {
        let _env = kimi_env();
        unsafe { std::env::set_var("PHEOBE_KIMI_BIN", "/no/such/kimi-xyz") };
        let dir = scratch("missing");
        let err = KimiWorker.run("x", &dir).unwrap_err();
        unsafe { std::env::remove_var("PHEOBE_KIMI_BIN") };
        assert!(err.to_string().contains("failed to spawn"));
        assert!(err.to_string().contains("PHEOBE_KIMI_BIN"));
    }

    /// PHEOBE-46: task `model` → `-m`; strict is refused (kimi needs the network).
    #[test]
    fn kimi_run_with_task_model_and_strict_refusal() {
        let _env = kimi_env();
        let dir = scratch("run-with");
        let (script, captured) = capture_kimi(&dir, r#"{"message":"ok"}"#);
        with_kimi_bin(&script, || {
            let ctx = crate::worker::WorkerCtx {
                model: Some("ds-deepseek-v4-flash".into()),
                sandbox: Some(crate::sandbox::Tier::Free),
                ..Default::default()
            };
            KimiWorker.run_with("p", &dir, &ctx).unwrap();
            let strict = crate::worker::WorkerCtx {
                sandbox: Some(crate::sandbox::Tier::Strict),
                ..Default::default()
            };
            let err = format!("{:#}", KimiWorker.run_with("p", &dir, &strict).unwrap_err());
            assert!(
                err.contains("kimi worker: sandbox 'strict' is not supported"),
                "{err}"
            );
        });
        let args = std::fs::read_to_string(&captured).unwrap();
        let lines: Vec<&str> = args.lines().collect();
        assert!(
            lines
                .windows(2)
                .any(|w| w == ["-m", "ds-deepseek-v4-flash"]),
            "{args}"
        );
    }
}
