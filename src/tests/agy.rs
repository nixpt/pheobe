//! PHEOBE-25: AGY worker adapter against a fake binary that records argv + cwd.
//! Env is process-global — every env-touching test holds `agy_env_lock`.

use super::{del_env, set_env};
use crate::worker::{worker_from_env, Worker};
use crate::worker_agy::{parse_stdout, sandbox_enabled, AgyWorker};

fn agy_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pheobe-agy-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fake_agy(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    crate::tests::write_shim(dir, name, body)
}

fn capture_agy(
    dir: &std::path::Path,
    response: &str,
    status: &str,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let capture = dir.join("captured.txt");
    let json = serde_json::json!({
        "status": status,
        "response": response,
        "usage": {
            "input_tokens": 100,
            "output_tokens": 50,
            "thinking_tokens": 25,
            "total_tokens": 175
        }
    });
    let body = format!(
        "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{}'\ndone\nprintf '%s\\n' \"CWD=$(pwd)\" >> '{}'\necho '{}'\n",
        capture.display(),
        capture.display(),
        json
    );
    let script = fake_agy(dir, "agy", &body);
    (script, capture)
}

struct AgyEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl AgyEnv {
    fn lock() -> Self {
        let guard = agy_env_lock();
        del_env("PHEOBE_AGY_BIN");
        del_env("PHEOBE_AGY_FLAGS");
        del_env("PHEOBE_SANDBOX");
        Self { _guard: guard }
    }
}

impl Drop for AgyEnv {
    fn drop(&mut self) {
        del_env("PHEOBE_AGY_BIN");
        del_env("PHEOBE_AGY_FLAGS");
        del_env("PHEOBE_SANDBOX");
    }
}

#[test]
fn agy_sandbox_tier_maps_to_the_cli_flag() {
    assert!(sandbox_enabled("strict").unwrap());
    assert!(!sandbox_enabled("moderate").unwrap());
    assert!(!sandbox_enabled("free").unwrap());
    let err = format!("{:#}", sandbox_enabled("unsupported").unwrap_err());
    assert!(
        err.contains("unknown PHEOBE_SANDBOX tier 'unsupported'"),
        "got: {err}"
    );
}

#[test]
fn agy_argv_shape_cwd_and_prompt_reach_the_binary() {
    let _env = AgyEnv::lock();
    let dir = scratch("argv");
    let (script, capture) = capture_agy(&dir, "all good", "SUCCESS");
    set_env("PHEOBE_AGY_BIN", &script.to_string_lossy());
    let out = AgyWorker.run("build the kernel", &dir).unwrap();
    assert_eq!(out.final_text, "all good");
    assert_eq!(out.tokens, Some(175));

    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert_eq!(lines[0], "-p");
    assert_eq!(lines[1], "build the kernel");
    assert_eq!(lines[2], "--output-format");
    assert_eq!(lines[3], "json");
    assert!(lines.contains(&"--dangerously-skip-permissions"));
    assert!(!lines.contains(&"--sandbox"));
    assert!(
        lines.contains(&format!("CWD={}", dir.display()).as_str()),
        "expected CWD to match worktree, got:\n{captured}"
    );
}

#[test]
fn agy_strict_tier_passes_sandbox_flag() {
    let _env = AgyEnv::lock();
    let dir = scratch("strict");
    let (script, capture) = capture_agy(&dir, "sandboxed", "SUCCESS");
    set_env("PHEOBE_AGY_BIN", &script.to_string_lossy());
    set_env("PHEOBE_SANDBOX", "strict");
    let out = AgyWorker.run("test sandboxing", &dir).unwrap();
    assert_eq!(out.final_text, "sandboxed");

    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert!(lines.contains(&"--sandbox"));
}

#[test]
fn agy_flags_default_is_replaced_not_appended() {
    let _env = AgyEnv::lock();
    let dir = scratch("flags");
    let (script, capture) = capture_agy(&dir, "flagged", "SUCCESS");
    set_env("PHEOBE_AGY_BIN", &script.to_string_lossy());
    set_env("PHEOBE_AGY_FLAGS", "--custom-flag");
    let out = AgyWorker.run("test flags", &dir).unwrap();
    assert_eq!(out.final_text, "flagged");

    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert!(lines.contains(&"--custom-flag"));
    assert!(!lines.contains(&"--dangerously-skip-permissions"));
}

#[test]
fn agy_failed_run_surfaces_the_stderr_tail() {
    let _env = AgyEnv::lock();
    let dir = scratch("fail");
    let script = fake_agy(&dir, "agy", "echo 'auth failed: invalid token' >&2; exit 2");
    set_env("PHEOBE_AGY_BIN", &script.to_string_lossy());
    let err = format!("{:#}", AgyWorker.run("task", &dir).unwrap_err());
    assert!(
        err.contains("auth failed: invalid token"),
        "expected stderr in error, got: {err}"
    );
    assert!(
        err.contains("exited with exit status: 2"),
        "expected status in error, got: {err}"
    );
}

#[test]
fn agy_status_error_surfaces_as_failure() {
    let _env = AgyEnv::lock();
    let dir = scratch("status-err");
    let (script, _) = capture_agy(&dir, "internal model error", "ERROR");
    set_env("PHEOBE_AGY_BIN", &script.to_string_lossy());
    let err = format!("{:#}", AgyWorker.run("task", &dir).unwrap_err());
    assert!(
        err.contains("reported failure status: internal model error"),
        "expected failure status message, got: {err}"
    );
}

#[test]
fn agy_tokens_extracted_from_the_result_object() {
    let raw = r#"{
        "status": "SUCCESS",
        "response": "completed",
        "usage": { "total_tokens": 1234 }
    }"#;
    let (out, is_err) = parse_stdout(raw);
    assert!(!is_err);
    assert_eq!(out.final_text, "completed");
    assert_eq!(out.tokens, Some(1234));
}

#[test]
fn agy_tokens_fallback_sum() {
    let raw = r#"{
        "status": "SUCCESS",
        "response": "done",
        "usage": {
            "input_tokens": 200,
            "output_tokens": 50,
            "thinking_tokens": 30
        }
    }"#;
    let (out, is_err) = parse_stdout(raw);
    assert!(!is_err);
    assert_eq!(out.tokens, Some(280));
}

#[test]
fn agy_json_report_contract_becomes_the_json_tail() {
    let raw = r#"{
        "status": "SUCCESS",
        "response": "Here is the result:\n```json\n{\"ok\":true,\"summary\":\"all tests pass\",\"doubts\":[]}\n```"
    }"#;
    let (out, _) = parse_stdout(raw);
    let tail = out.json_tail.expect("expected json_tail");
    assert_eq!(tail["ok"], true);
    assert_eq!(tail["summary"], "all tests pass");
}

#[test]
fn agy_unparseable_stdout_degrades_to_prose() {
    let raw = "plain unformatted text output with no json";
    let (out, is_err) = parse_stdout(raw);
    assert!(!is_err);
    assert_eq!(out.final_text, raw);
    assert_eq!(out.tokens, None);
    assert_eq!(out.json_tail, None);
}

#[test]
fn agy_missing_binary_errors_clearly() {
    let _env = AgyEnv::lock();
    let dir = scratch("missing");
    set_env("PHEOBE_AGY_BIN", "/nonexistent/agy/binary");
    let err = format!("{:#}", AgyWorker.run("task", &dir).unwrap_err());
    assert!(
        err.contains("failed to spawn '/nonexistent/agy/binary'"),
        "got: {err}"
    );
    assert!(err.contains("set PHEOBE_AGY_BIN to override"));
}

#[test]
fn agy_registry_resolves() {
    assert!(
        matches!(worker_from_env("agy"), Ok(Some(_))),
        "agy = its adapter (PHEOBE-25)"
    );
    assert!(
        matches!(worker_from_env("antigravity"), Ok(Some(_))),
        "antigravity = alias for agy adapter (PHEOBE-25)"
    );
}

/// PHEOBE-46: task `model` → `--model`; strict still uses agy's native `--sandbox`.
#[test]
fn agy_run_with_task_model_and_native_strict() {
    let _env = AgyEnv::lock();
    let dir = scratch("run-with");
    let (script, capture) = capture_agy(&dir, "ok", "SUCCESS");
    set_env("PHEOBE_AGY_BIN", &script.to_string_lossy());
    let ctx = crate::worker::WorkerCtx {
        model: Some("gemini-3-pro".into()),
        sandbox: Some(crate::sandbox::Tier::Strict),
        ..Default::default()
    };
    AgyWorker.run_with("p", &dir, &ctx).unwrap();
    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert!(
        lines.contains(&"--sandbox"),
        "strict → agy's own sandbox: {captured}"
    );
    assert!(
        lines.windows(2).any(|w| w == ["--model", "gemini-3-pro"]),
        "{captured}"
    );
}
