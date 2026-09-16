//! PHEOBE-5: the claude worker adapter against a fake binary that records
//! argv + cwd and emits canned stdout. Env is process-global — every
//! env-touching test holds the shared claude_env_lock.

use super::{claude_env_lock, del_env, set_env};
use crate::task::Task;
use crate::worker::Worker;

// ── PHEOBE-5: claude worker adapter (PHEOBE_PROVIDER=claude) ────────────────

/// Fake claude binary: dumps its argv (one per line) + cwd to a file, then
/// emits the canned stdout and exits with a canned code. The env vars the
/// script reads are process-global, so every claude_ test holds one lock —
/// these tests would otherwise race each other (set_var is process-wide).
struct FakeClaude {
    dir: std::path::PathBuf,
    script: std::path::PathBuf,
}

impl FakeClaude {
    fn new(name: &str, stdout: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("pheobe-claude-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let stdout_path = dir.join("stdout.txt");
        std::fs::write(&stdout_path, stdout).unwrap();
        let script = dir.join("fake-claude");
        let stdout_ref = stdout_path.display().to_string();
        std::fs::write(
            &script,
            format!(
                r#"#!/bin/sh
for a in "$@"; do printf '%s\n' "$a" >> "$PHEOBE_FAKE_ARGV"; done
pwd >> "$PHEOBE_FAKE_ARGV"
if [ -n "$PHEOBE_FAKE_SLEEP" ]; then sleep "$PHEOBE_FAKE_SLEEP"; fi
cat "{stdout_ref}"
if [ -n "$PHEOBE_FAKE_EXIT" ]; then echo "fake claude exploded" >&2; exit "$PHEOBE_FAKE_EXIT"; fi
"#
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        FakeClaude { dir, script }
    }

    fn argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("argv.txt"))
            .unwrap()
            .lines()
            .map(|s| s.to_string())
            .collect()
    }
}

/// Run the real ClaudeWorker against a fake binary. Sets the env the worker
/// reads; leaves PHEOBE_FAKE_EXIT / PHEOBE_FAKE_SLEEP for the caller to set
/// before calling. Cleans every PHEOBE_* var it touched.
fn run_fake(f: &FakeClaude, prompt: &str) -> anyhow::Result<crate::worker::WorkerOutcome> {
    let argv_path = f.dir.join("argv.txt");
    let _ = std::fs::remove_file(&argv_path);
    set_env("PHEOBE_CLAUDE_BIN", &f.script.display().to_string());
    set_env("PHEOBE_FAKE_ARGV", &argv_path.display().to_string());
    let out = crate::worker_claude::ClaudeWorker.run(prompt, &f.dir);
    del_env("PHEOBE_CLAUDE_BIN");
    del_env("PHEOBE_FAKE_ARGV");
    del_env("PHEOBE_CLAUDE_FLAGS");
    del_env("PHEOBE_CLAUDE_TIMEOUT_SECS");
    del_env("PHEOBE_FAKE_SLEEP");
    del_env("PHEOBE_FAKE_EXIT");
    out
}

/// (a) the argv contract: -p <prompt> --output-format json, default flags
/// appended, cwd = the worktree.
#[test]
fn claude_argv_shape_default_flags_and_cwd() {
    let _g = claude_env_lock();
    let f = FakeClaude::new(
        "argv",
        r#"{"type":"result","result":"ok","usage":{"input_tokens":1,"output_tokens":2}}"#,
    );
    let wt = f.dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    // run with cwd = the wt subdirectory
    let argv_path = f.dir.join("argv.txt");
    let _ = std::fs::remove_file(&argv_path);
    set_env("PHEOBE_CLAUDE_BIN", &f.script.display().to_string());
    set_env("PHEOBE_FAKE_ARGV", &argv_path.display().to_string());
    let out = crate::worker_claude::ClaudeWorker.run("the whole prompt", &wt);
    del_env("PHEOBE_CLAUDE_BIN");
    del_env("PHEOBE_FAKE_ARGV");
    del_env("PHEOBE_CLAUDE_FLAGS");
    let out = out.unwrap();

    let argv = f.argv();
    assert_eq!(
        &argv[..4],
        &["-p", "the whole prompt", "--output-format", "json"]
    );
    assert!(
        argv.contains(&"--dangerously-skip-permissions".to_string()),
        "default PHEOBE_CLAUDE_FLAGS must apply, got: {argv:?}"
    );
    assert_eq!(
        argv.last().unwrap(),
        &wt.display().to_string(),
        "cwd must be the worktree"
    );
    assert_eq!(out.final_text, "ok");
    assert_eq!(out.tokens, Some(3), "input+output summed");
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (a continued) PHEOBE_CLAUDE_FLAGS REPLACES the default entirely — the ccf
/// mode appends nothing else; the caller's env carries the flownet auth.
#[test]
fn claude_flags_override_replaces_default() {
    let _g = claude_env_lock();
    let f = FakeClaude::new("flags", r#"{"type":"result","result":"ok"}"#);
    set_env("PHEOBE_CLAUDE_FLAGS", "ccf-mode --model opus");
    let out = run_fake(&f, "p").unwrap();
    assert_eq!(out.final_text, "ok");

    let argv = f.argv();
    assert!(argv.contains(&"ccf-mode".to_string()), "got: {argv:?}");
    assert!(argv.contains(&"--model".to_string()) && argv.contains(&"opus".to_string()));
    assert!(
        !argv.contains(&"--dangerously-skip-permissions".to_string()),
        "override must REPLACE the default, got: {argv:?}"
    );
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (b) the single-JSON shape (`--output-format json`): final_text from
/// `result`, tokens from the usage buckets, usd from total_cost_usd, and a
/// JSON-object final message handed up as json_tail.
#[test]
fn claude_single_json_with_usage_and_cost() {
    let _g = claude_env_lock();
    let f = FakeClaude::new(
        "single",
        concat!(
            r#"{"type":"result","subtype":"success","is_error":false,"#,
            r#""total_cost_usd":0.0125,"#,
            r#""usage":{"input_tokens":100,"output_tokens":50,"#,
            r#""cache_read_input_tokens":7,"cache_creation_input_tokens":3},"#,
            r#""result":"{\"ok\":true,\"summary\":\"did the thing\",\"next_steps\":[\"review\"],\"doubts\":[\"assumed x\"]}"}"#
        ),
    );
    let out = run_fake(&f, "p").unwrap();
    assert!(
        out.json_tail.is_some(),
        "engine JSON handoff must surface as json_tail"
    );
    let tail = out.json_tail.unwrap();
    assert_eq!(tail["summary"], "did the thing");
    assert_eq!(out.tokens, Some(160));
    assert!((out.usd.unwrap() - 0.0125).abs() < 1e-9);
    assert!(out.final_text.contains("did the thing"));
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (b continued) the same outcome pushed through the real run_worker path:
/// the tail merges ONLY the contract keys (summary/next_steps/doubts) —
/// identical normalization to the opencode adapter.
#[test]
fn claude_json_tail_merges_through_run_worker() {
    use crate::agent::{run_worker, LoopCfg};
    let _g = claude_env_lock();
    let f = FakeClaude::new(
        "tail",
        concat!(
            r#"{"type":"result","result":"{\"ok\":true,\"summary\":\"did the thing\",\"next_steps\":[\"review\"],\"doubts\":[\"assumed x\"]}","#,
            r#""usage":{"input_tokens":100,"output_tokens":50,"#,
            r#""cache_read_input_tokens":7,"cache_creation_input_tokens":3}}"#
        ),
    );
    set_env("PHEOBE_CLAUDE_BIN", &f.script.display().to_string());
    set_env(
        "PHEOBE_FAKE_ARGV",
        &f.dir.join("argv.txt").display().to_string(),
    );
    let task: Task =
        serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#)
            .unwrap();
    let out = run_worker(
        &crate::worker_claude::ClaudeWorker,
        &task,
        &f.dir,
        "claude-tail",
        "",
        &[],
        &LoopCfg::default(),
    )
    .unwrap();
    del_env("PHEOBE_CLAUDE_BIN");
    del_env("PHEOBE_FAKE_ARGV");
    assert!(out.ok);
    assert_eq!(out.summary.as_deref(), Some("did the thing"));
    assert_eq!(out.next_steps, vec!["review"]);
    assert_eq!(out.doubts, vec!["assumed x"]);
    assert_eq!(out.usage.total_tokens, Some(160));
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (c) the stream shape (defensive): a stream of objects must parse; the
/// last result-typed object wins.
#[test]
fn claude_stream_of_objects_parses_defensively() {
    let _g = claude_env_lock();
    let f = FakeClaude::new(
        "stream",
        concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"session_id\":\"abc\"}\n",
            "{\"type\":\"assistant\",\"message\":{\"role\":\"assistant\"}}\n",
            "{\"type\":\"result\",\"result\":\"stream final text\",",
            "\"usage\":{\"input_tokens\":10,\"output_tokens\":5},\"total_cost_usd\":0.5}\n"
        ),
    );
    let out = run_fake(&f, "p").unwrap();
    assert_eq!(out.final_text, "stream final text");
    assert_eq!(out.tokens, Some(15));
    assert!((out.usd.unwrap() - 0.5).abs() < 1e-9);
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (c continued) prose fallback: an unparseable tail means the whole stdout
/// IS the engine's prose — no tokens, no json_tail, no crash.
#[test]
fn claude_unparseable_stdout_becomes_prose() {
    let _g = claude_env_lock();
    let f = FakeClaude::new("prose", "just some prose\nsecond line");
    let out = run_fake(&f, "p").unwrap();
    assert_eq!(out.final_text, "just some prose\nsecond line");
    assert_eq!(out.tokens, None);
    assert_eq!(out.usd, None);
    assert_eq!(out.json_tail, None);
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (d) a missing binary must error clearly, naming the culprit path.
#[test]
fn claude_missing_binary_errors_clearly() {
    let _g = claude_env_lock();
    set_env("PHEOBE_CLAUDE_BIN", "/nonexistent/pheobe-fake-claude");
    let dir = std::env::temp_dir();
    let err = crate::worker_claude::ClaudeWorker
        .run("p", &dir)
        .unwrap_err();
    del_env("PHEOBE_CLAUDE_BIN");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("/nonexistent/pheobe-fake-claude"),
        "got: {msg}"
    );
    assert!(msg.contains("failed to spawn"), "got: {msg}");
}

/// (d continued) a nonzero exit must surface the code + the engine's stderr.
#[test]
fn claude_nonzero_exit_is_a_clear_error() {
    let _g = claude_env_lock();
    let f = FakeClaude::new("exit", "partial stdout");
    set_env("PHEOBE_FAKE_EXIT", "3");
    let err = run_fake(&f, "p").unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("exited with exit status: 3"), "got: {msg}");
    assert!(
        msg.contains("fake claude exploded"),
        "stderr tail missing: {msg}"
    );
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (e) the adapter's own subprocess timeout kills a hung child.
#[test]
fn claude_timeout_kills_the_child() {
    let _g = claude_env_lock();
    let f = FakeClaude::new("timeout", "never happens");
    set_env("PHEOBE_CLAUDE_TIMEOUT_SECS", "1");
    set_env("PHEOBE_FAKE_SLEEP", "10");
    let err = run_fake(&f, "p").unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("timed out"), "got: {msg}");
    assert!(msg.contains("killed"), "got: {msg}");
    std::fs::remove_dir_all(&f.dir).ok();
}

/// (f) is_error on the result object is a hard error, not a silent ok:true.
#[test]
fn claude_is_error_surfaces_as_failure() {
    let _g = claude_env_lock();
    let f = FakeClaude::new(
        "iserr",
        r#"{"type":"result","is_error":true,"result":"credit balance too low"}"#,
    );
    let err = run_fake(&f, "p").unwrap_err();
    let msg = format!("{err:#}");
    assert!(msg.contains("is_error"), "got: {msg}");
    assert!(
        msg.contains("credit balance too low"),
        "engine text missing: {msg}"
    );
    std::fs::remove_dir_all(&f.dir).ok();
}
