//! PHEOBE-6 / PHEOBE-23: cursor worker adapter against a fake binary that
//! records argv + cwd. Env is process-global — every env-touching test
//! holds `cursor_env_lock`.

use super::{del_env, set_env};
use crate::worker::Worker;
use crate::worker_cursor::{parse_stdout, sandbox_flag, CursorWorker};

fn cursor_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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

fn capture_cursor(
    dir: &std::path::Path,
    final_text: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
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

struct CursorEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl CursorEnv {
    fn lock() -> Self {
        let guard = cursor_env_lock();
        del_env("PHEOBE_CURSOR_BIN");
        del_env("PHEOBE_CURSOR_FLAGS");
        del_env("PHEOBE_SANDBOX");
        Self { _guard: guard }
    }
}

impl Drop for CursorEnv {
    fn drop(&mut self) {
        del_env("PHEOBE_CURSOR_BIN");
        del_env("PHEOBE_CURSOR_FLAGS");
        del_env("PHEOBE_SANDBOX");
    }
}

#[test]
fn cursor_sandbox_tier_maps_to_the_cli_flag() {
    assert_eq!(sandbox_flag("strict").unwrap(), "enabled");
    assert_eq!(sandbox_flag("moderate").unwrap(), "enabled");
    assert_eq!(sandbox_flag("free").unwrap(), "disabled");
    let err = format!("{:#}", sandbox_flag("chaos").unwrap_err());
    assert!(
        err.contains("unknown PHEOBE_SANDBOX tier 'chaos'"),
        "got: {err}"
    );
}

#[test]
fn cursor_argv_shape_cwd_sandbox_and_prompt_reach_the_binary() {
    let _env = CursorEnv::lock();
    let dir = scratch("argv");
    let (script, capture) = capture_cursor(&dir, "ok");
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    let out = CursorWorker.run("do the thing", &dir).unwrap();
    assert_eq!(out.final_text, "ok");
    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert_eq!(lines[0], "-p");
    assert_eq!(lines[1], "--output-format");
    assert_eq!(lines[2], "json");
    assert_eq!(lines[3], "--sandbox");
    assert_eq!(
        lines[4], "enabled",
        "unset PHEOBE_SANDBOX defaults to moderate"
    );
    assert_eq!(lines[5], "--yolo", "default flags apply");
    assert_eq!(lines[6], "--trust");
    assert_eq!(lines[7], "do the thing", "prompt is one argv element");
    assert!(
        !captured.contains("--worktree"),
        "pheobe already provisioned the worktree; nested cursor --worktree is a trap"
    );
    assert_eq!(
        lines[lines.len() - 1],
        &format!("CWD={}", dir.display()),
        "cwd = worktree"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_free_tier_disables_the_cli_sandbox() {
    let _env = CursorEnv::lock();
    let dir = scratch("free");
    let (script, capture) = capture_cursor(&dir, "ok");
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    set_env("PHEOBE_SANDBOX", "free");
    CursorWorker.run("x", &dir).unwrap();
    let captured = std::fs::read_to_string(&capture).unwrap();
    assert!(
        captured.contains("--sandbox\ndisabled"),
        "free → --sandbox disabled, got:\n{captured}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_flags_default_is_replaced_not_appended() {
    let _env = CursorEnv::lock();
    let dir = scratch("flags");
    let (script, capture) = capture_cursor(&dir, "ok");
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    set_env("PHEOBE_CURSOR_FLAGS", "ccf-mode");
    CursorWorker.run("x", &dir).unwrap();
    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert_eq!(lines[5], "ccf-mode", "replaces the default entirely");
    assert!(!captured.contains("--yolo"), "default gone when overridden");
    assert!(
        captured.contains("--sandbox\nenabled"),
        "sandbox is policy, not a flag, still present"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_tokens_extracted_from_the_result_object() {
    let _env = CursorEnv::lock();
    let dir = scratch("tokens");
    let script = fake_cursor(
        dir.as_path(),
        "cursor-agent",
        r#"echo '{"content":"done","usage":{"total_tokens":42}}'"#,
    );
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    let out = CursorWorker.run("x", &dir).unwrap();
    assert_eq!(out.final_text, "done");
    assert_eq!(out.tokens, Some(42), "usage.total_tokens resolves");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_sdk_camelcase_usage_and_charged_cents_become_tokens_and_usd() {
    // @cursor/sdk TokenUsage.totalTokens + UsageCost.chargedCents (cents → USD).
    let v = parse_stdout(
        r#"{"content":"done","usage":{"totalTokens":91,"cost":{"chargedCents":42,"rawCostCents":50}}}"#,
    );
    assert_eq!(v.tokens, Some(91));
    assert_eq!(v.usd, Some(0.42));
}

#[test]
fn cursor_stream_of_objects_last_wins() {
    let _env = CursorEnv::lock();
    let dir = scratch("stream");
    let script = fake_cursor(
        dir.as_path(),
        "cursor-agent",
        "echo '{\"content\":\"first\"}'; echo '{\"content\":\"second\",\"usage\":{\"tokens\":9}}'",
    );
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    let out = CursorWorker.run("x", &dir).unwrap();
    assert_eq!(out.final_text, "second", "last object wins");
    assert_eq!(out.tokens, Some(9));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_unparseable_stdout_degrades_to_prose() {
    let _env = CursorEnv::lock();
    let dir = scratch("prose");
    let script = fake_cursor(
        dir.as_path(),
        "cursor-agent",
        "echo 'the fix landed — parser now handles nested blocks'",
    );
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    let out = CursorWorker.run("x", &dir).unwrap();
    assert!(out.final_text.contains("the fix landed"), "prose fallback");
    assert_eq!(out.tokens, None);
    assert_eq!(out.json_tail, None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_json_report_contract_becomes_the_json_tail() {
    let _env = CursorEnv::lock();
    let dir = scratch("tail");
    let script = fake_cursor(
        dir.as_path(),
        "cursor-agent",
        r#"echo '{"content":"{\"ok\":true,\"summary\":\"landed\"}"}'"#,
    );
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    let out = CursorWorker.run("x", &dir).unwrap();
    assert_eq!(out.final_text, "{\"ok\":true,\"summary\":\"landed\"}");
    let tail = out
        .json_tail
        .expect("report-contract object becomes the tail");
    assert_eq!(tail["ok"], true);
    assert_eq!(tail["summary"], "landed");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_missing_binary_errors_clearly() {
    let _env = CursorEnv::lock();
    let dir = scratch("missing");
    set_env("PHEOBE_CURSOR_BIN", "/nonexistent/pheobe-fake-cursor");
    let err = format!("{:#}", CursorWorker.run("x", &dir).unwrap_err());
    assert!(
        err.contains("failed to spawn '/nonexistent/pheobe-fake-cursor'"),
        "got: {err}"
    );
    assert!(
        err.contains("PHEOBE_CURSOR_BIN"),
        "names the env override, got: {err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_failed_run_surfaces_the_stderr_tail() {
    let _env = CursorEnv::lock();
    let dir = scratch("failed");
    let script = fake_cursor(
        dir.as_path(),
        "cursor-agent",
        "echo 'nope: cursor auth expired' >&2; exit 1",
    );
    set_env("PHEOBE_CURSOR_BIN", &script.to_string_lossy());
    let err = format!("{:#}", CursorWorker.run("x", &dir).unwrap_err());
    assert!(err.contains("exited with"), "got: {err}");
    assert!(
        err.contains("cursor auth expired"),
        "stderr tail included, got: {err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn cursor_unknown_sandbox_tier_errors_at_run() {
    let _env = CursorEnv::lock();
    let dir = scratch("bad-tier");
    set_env("PHEOBE_SANDBOX", "chaos");
    let err = format!("{:#}", CursorWorker.run("x", &dir).unwrap_err());
    assert!(
        err.contains("unknown PHEOBE_SANDBOX tier 'chaos'"),
        "got: {err}"
    );
    std::fs::remove_dir_all(&dir).ok();
}
