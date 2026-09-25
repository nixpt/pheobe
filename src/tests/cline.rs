//! PHEOBE-49: cline worker adapter against a fake binary that records argv + cwd,
//! plus the stream parser against cline 3.0.62's real `--json` shapes.
//! Env is process-global — every env-touching test holds `cline_env_lock`.

use super::{del_env, set_env};
use crate::worker::{worker_from_env, Worker, WorkerCtx};
use crate::worker_cline::{cline_args, parse_stdout, ClineOpts, ClineWorker};

fn cline_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

const ENV: [&str; 6] = [
    "PHEOBE_CLINE_BIN",
    "PHEOBE_CLINE_PROVIDER",
    "PHEOBE_CLINE_MODEL",
    "PHEOBE_CLINE_DATA_DIR",
    "PHEOBE_CLINE_FLAGS",
    "PHEOBE_CLINE_TIMEOUT_SECS",
];

struct ClineEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl ClineEnv {
    fn lock() -> Self {
        let g = cline_env_lock();
        ENV.iter().for_each(|k| del_env(k));
        Self { _guard: g }
    }
}

impl Drop for ClineEnv {
    fn drop(&mut self) {
        ENV.iter().for_each(|k| del_env(k));
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pheobe-cline-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `run_result` line as cline 3.0.62 prints it.
fn run_result(finish: &str, text: &str) -> String {
    serde_json::json!({
        "ts": "2026-09-25T00:16:37.626Z",
        "type": "run_result",
        "finishReason": finish,
        "iterations": 3,
        "usage": {"inputTokens": 1, "outputTokens": 1, "cacheReadTokens": 0,
                  "cacheWriteTokens": 0, "totalCost": 0.0},
        "aggregateUsage": {"inputTokens": 100, "outputTokens": 40, "cacheReadTokens": 7,
                           "cacheWriteTokens": 3, "totalCost": 0.012},
        "durationMs": 211,
        "text": text,
        "model": {"id": "deepseek/deepseek-v4-flash", "provider": "openai"}
    })
    .to_string()
}

/// A fake cline: records argv + cwd, prints a hook event and `stdout_line`,
/// exits `code`.
fn capture_cline(
    dir: &std::path::Path,
    stdout_line: &str,
    code: i32,
) -> (String, std::path::PathBuf) {
    let capture = dir.join("captured.txt");
    let body = format!(
        "for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{c}'\ndone\n\
         printf '%s\\n' \"CWD=$(pwd)\" >> '{c}'\n\
         echo '{{\"type\":\"hook_event\",\"hookEventName\":\"agent_start\"}}'\n\
         cat <<'EOF'\n{line}\nEOF\nexit {code}\n",
        c = capture.display(),
        line = stdout_line,
    );
    let script = crate::tests::write_shim(dir, "cline", &body);
    (script.to_string_lossy().into_owned(), capture)
}

#[test]
fn cline_is_a_registered_provider() {
    assert!(worker_from_env("cline").unwrap().is_some());
    assert!(crate::worker::provider_names().contains(&"cline"));
}

#[test]
fn cline_args_headless_shape() {
    let wt = std::path::Path::new("/wt");
    let bare = cline_args("do x", wt, &ClineOpts::default());
    assert_eq!(
        bare,
        ["do x", "--json", "--auto-approve", "true", "-c", "/wt"]
    );
    let full = cline_args(
        "do x",
        wt,
        &ClineOpts {
            provider: Some("openai".into()),
            model: Some("m1".into()),
            data_dir: Some("/d".into()),
            flags: "--thinking low".into(),
            timeout_secs: 90,
        },
    );
    for pair in [
        ["-t", "90"],
        ["-P", "openai"],
        ["-m", "m1"],
        ["--data-dir", "/d"],
    ] {
        assert!(full.windows(2).any(|w| w == pair), "{pair:?} in {full:?}");
    }
    assert_eq!(
        &full[full.len() - 2..],
        ["--thinking", "low"],
        "extra flags last"
    );
}

#[test]
fn parse_reads_the_last_run_result() {
    let stream = format!(
        "{{\"type\":\"agent_event\",\"event\":{{\"type\":\"iteration_start\"}}}}\n{}\n",
        run_result(
            "completed",
            "done.\n```json\n{\"ok\":true,\"summary\":\"added b.txt\"}\n```"
        )
    );
    let (o, err) = parse_stdout(&stream);
    assert_eq!(err, None);
    assert!(o.final_text.starts_with("done."));
    assert_eq!(o.tokens, Some(150), "aggregateUsage: 100+40+7+3");
    assert_eq!(o.usd, Some(0.012));
    assert_eq!(o.turns, Some(3));
    assert_eq!(o.json_tail.unwrap()["summary"], "added b.txt");
}

#[test]
fn parse_finish_error_is_a_failure_with_the_text() {
    // cline 3.0.62's real quota failure, trimmed.
    let (_, err) = parse_stdout(&run_result(
        "error",
        "You have reached your monthly Clinepass limit.",
    ));
    assert_eq!(
        err.as_deref(),
        Some("You have reached your monthly Clinepass limit.")
    );
    // an error event with no run_result is still a failure, not prose
    let (_, err) = parse_stdout("{\"type\":\"error\",\"message\":\"boom\"}\n");
    assert_eq!(err.as_deref(), Some("boom"));
    // no JSON at all degrades to prose, not an error
    let (o, err) = parse_stdout("plain words\n");
    assert_eq!((o.final_text.as_str(), err), ("plain words\n", None));
}

#[test]
fn cline_run_passes_argv_cwd_and_env_knobs() {
    let _env = ClineEnv::lock();
    let dir = scratch("run");
    let (bin, capture) = capture_cline(&dir, &run_result("completed", "all good"), 0);
    set_env("PHEOBE_CLINE_BIN", &bin);
    set_env("PHEOBE_CLINE_PROVIDER", "openai");
    set_env("PHEOBE_CLINE_MODEL", "env-model");
    let ctx = WorkerCtx {
        model: Some("task-model".into()),
        ttl: Some(std::time::Duration::from_secs(120)),
        ..Default::default()
    };
    let out = ClineWorker.run_with("the prompt", &dir, &ctx).unwrap();
    assert_eq!(out.final_text, "all good");
    assert_eq!(out.turns, Some(3));
    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert_eq!(lines[0], "the prompt");
    assert!(
        lines.windows(2).any(|w| w == ["-c", dir.to_str().unwrap()]),
        "{captured}"
    );
    assert!(
        lines.windows(2).any(|w| w == ["-m", "env-model"]),
        "env model wins: {captured}"
    );
    assert!(
        lines.windows(2).any(|w| w == ["-P", "openai"]),
        "{captured}"
    );
    assert!(
        lines.windows(2).any(|w| w == ["-t", "120"]),
        "ttl bounds cline's -t: {captured}"
    );
    let cwd = lines.iter().find(|l| l.starts_with("CWD=")).unwrap();
    assert_eq!(
        std::fs::canonicalize(&cwd[4..]).unwrap(),
        std::fs::canonicalize(&dir).unwrap()
    );
}

#[test]
fn cline_failed_run_surfaces_the_run_result_text() {
    let _env = ClineEnv::lock();
    let dir = scratch("quota");
    let (bin, _) = capture_cline(&dir, &run_result("error", "ClinePass limit reached"), 1);
    set_env("PHEOBE_CLINE_BIN", &bin);
    let err = ClineWorker.run("p", &dir).unwrap_err().to_string();
    assert!(err.contains("ClinePass limit reached"), "{err}");
}

#[test]
fn cline_nonzero_exit_without_result_is_an_error() {
    let _env = ClineEnv::lock();
    let dir = scratch("crash");
    let (bin, _) = capture_cline(&dir, "not json", 3);
    set_env("PHEOBE_CLINE_BIN", &bin);
    let err = ClineWorker.run("p", &dir).unwrap_err().to_string();
    assert!(err.contains("exited with"), "{err}");
}

#[test]
fn cline_strict_tier_is_refused() {
    let _env = ClineEnv::lock();
    let dir = scratch("strict");
    let (bin, capture) = capture_cline(&dir, &run_result("completed", "x"), 0);
    set_env("PHEOBE_CLINE_BIN", &bin);
    let ctx = WorkerCtx {
        sandbox: Some(crate::sandbox::Tier::Strict),
        ..Default::default()
    };
    let err = ClineWorker
        .run_with("p", &dir, &ctx)
        .unwrap_err()
        .to_string();
    assert!(err.contains("strict"), "{err}");
    assert!(!capture.exists(), "cline must not run under a refused tier");
}
