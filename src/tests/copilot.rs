//! PHEOBE-56: copilot worker adapter against a fake binary that records argv +
//! cwd and writes a usage file, plus the JSONL parser against Copilot CLI
//! 1.0.98's real `--output-format json` shapes.
//! Env is process-global — every env-touching test holds `copilot_env_lock`.

use super::{del_env, set_env};
use crate::worker::{worker_from_env, Worker, WorkerCtx};
use crate::worker_copilot::{
    copilot_args, parse_stdout, tokens_from_usage, usage_file, CopilotWorker,
};

fn copilot_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

const ENV: [&str; 4] = [
    "PHEOBE_COPILOT_BIN",
    "PHEOBE_COPILOT_MODEL",
    "PHEOBE_COPILOT_FLAGS",
    "PHEOBE_COPILOT_TIMEOUT_SECS",
];

struct CopilotEnv {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl CopilotEnv {
    fn lock() -> Self {
        let g = copilot_env_lock();
        ENV.iter().for_each(|k| del_env(k));
        Self { _guard: g }
    }
}

impl Drop for CopilotEnv {
    fn drop(&mut self) {
        ENV.iter().for_each(|k| del_env(k));
    }
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pheobe-copilot-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn event(kind: &str, data: serde_json::Value) -> String {
    serde_json::json!({"type": kind, "data": data, "id": "x", "timestamp": "t"}).to_string()
}

/// A trimmed Copilot CLI 1.0.98 stream: a tool-call message (empty content),
/// two turns, the final answer, then the `result` line.
fn stream(final_answer: &str, exit_code: i64) -> String {
    [
        event(
            "session.tools_updated",
            serde_json::json!({"model": "gpt-5.6-luna"}),
        ),
        event("assistant.turn_start", serde_json::json!({"turnId": "0"})),
        event(
            "assistant.message",
            serde_json::json!({"content": "", "toolRequests": [{"name": "apply_patch"}]}),
        ),
        event("assistant.turn_end", serde_json::json!({"turnId": "0"})),
        event(
            "assistant.message",
            serde_json::json!({"content": final_answer, "phase": "final_answer"}),
        ),
        event("assistant.turn_end", serde_json::json!({"turnId": "1"})),
        serde_json::json!({"type": "result", "exitCode": exit_code,
            "usage": {"premiumRequests": 1, "sessionDurationMs": 4670}})
        .to_string(),
    ]
    .join("\n")
}

/// The `--usage-output-file` payload, trimmed to what the adapter reads.
const USAGE: &str = r#"{"totalPremiumRequestCost":1,"tokenDetails":{"input":{"tokenCount":6},"cache_read":{"tokenCount":23992},"cache_write":{"tokenCount":4652},"output":{"tokenCount":66}},"modelMetrics":{"gpt-5.6-luna":{"usage":{"inputTokens":28650,"outputTokens":66}}}}"#;

/// A fake copilot: records argv + cwd, writes `USAGE` to the path following
/// `--usage-output-file`, prints `stdout`, exits `code`.
fn capture_copilot(dir: &std::path::Path, stdout: &str, code: i32) -> (String, std::path::PathBuf) {
    let capture = dir.join("captured.txt");
    let body = format!(
        "usage=''; prev=''\n\
         for a in \"$@\"; do printf '%s\\n' \"$a\" >> '{c}'\n\
         [ \"$prev\" = --usage-output-file ] && usage=\"$a\"; prev=\"$a\"\ndone\n\
         printf '%s\\n' \"CWD=$(pwd)\" >> '{c}'\n\
         [ -n \"$usage\" ] && printf '%s' '{usage}' > \"$usage\"\n\
         cat <<'EOF'\n{stdout}\nEOF\nexit {code}\n",
        c = capture.display(),
        usage = USAGE,
    );
    let script = crate::tests::write_shim(dir, "copilot", &body);
    (script.to_string_lossy().into_owned(), capture)
}

#[test]
fn copilot_is_a_registered_provider() {
    assert!(worker_from_env("copilot").unwrap().is_some());
    assert!(crate::worker::provider_names().contains(&"copilot"));
}

#[test]
fn copilot_args_headless_shape() {
    let wt = std::path::Path::new("/wt");
    let bare = copilot_args("do x", wt, None, None, "");
    assert_eq!(
        bare,
        [
            "-p",
            "do x",
            "--output-format",
            "json",
            "--no-ask-user",
            "-C",
            "/wt",
            "--deny-tool=shell(git push)",
            "--deny-tool=shell(gh:*)",
        ]
    );
    let full = copilot_args(
        "do x",
        wt,
        Some("gpt-5.6-luna"),
        Some(std::path::Path::new("/wt/.pheobe/copilot-usage.json")),
        "--allow-all-tools --disable-builtin-mcps",
    );
    for pair in [
        ["--model", "gpt-5.6-luna"],
        ["--usage-output-file", "/wt/.pheobe/copilot-usage.json"],
    ] {
        assert!(full.windows(2).any(|w| w == pair), "{pair:?} in {full:?}");
    }
    assert_eq!(
        &full[full.len() - 2..],
        ["--allow-all-tools", "--disable-builtin-mcps"],
        "flags last"
    );
}

#[test]
fn parse_reads_the_last_non_empty_message_and_counts_turns() {
    let (o, err) = parse_stdout(&stream(
        "done.\n```json\n{\"ok\":true,\"summary\":\"added b.txt\"}\n```",
        0,
    ));
    assert_eq!(err, None);
    assert!(o.final_text.starts_with("done."), "{}", o.final_text);
    assert_eq!(o.turns, Some(2));
    assert_eq!(o.usd, None, "copilot bills premium requests, not USD");
    assert_eq!(o.json_tail.unwrap()["summary"], "added b.txt");
}

#[test]
fn parse_nonzero_exit_and_error_events_are_failures() {
    let (_, err) = parse_stdout(&stream("partial", 1));
    assert_eq!(err.as_deref(), Some("result exitCode=1"));
    let with_error = format!(
        "{}\n{}",
        event(
            "session.error",
            serde_json::json!({"message": "quota exceeded"})
        ),
        serde_json::json!({"type": "result", "exitCode": 1})
    );
    let (_, err) = parse_stdout(&with_error);
    assert_eq!(err.as_deref(), Some("quota exceeded"));
    // an error event and no result is still a failure, not prose
    let (_, err) = parse_stdout(&event(
        "session.error",
        serde_json::json!({"message": "boom"}),
    ));
    assert_eq!(err.as_deref(), Some("boom"));
    // no JSON at all degrades to prose, not an error
    let (o, err) = parse_stdout("plain words\n");
    assert_eq!((o.final_text.as_str(), err), ("plain words\n", None));
}

#[test]
fn usage_file_tokens_sum_every_class() {
    let u: serde_json::Value = serde_json::from_str(USAGE).unwrap();
    assert_eq!(tokens_from_usage(&u), Some(6 + 23992 + 4652 + 66));
    // older shape without tokenDetails: per-model input + output
    let u = serde_json::json!({"modelMetrics": {"a": {"usage": {"inputTokens": 10, "outputTokens": 5}},
                                                "b": {"usage": {"inputTokens": 1, "outputTokens": 1}}}});
    assert_eq!(tokens_from_usage(&u), Some(17));
    assert_eq!(tokens_from_usage(&serde_json::json!({})), None);
}

#[test]
fn copilot_run_passes_argv_cwd_env_knobs_and_reads_usage() {
    let _env = CopilotEnv::lock();
    let dir = scratch("run");
    let (bin, capture) = capture_copilot(&dir, &stream("all good", 0), 0);
    set_env("PHEOBE_COPILOT_BIN", &bin);
    set_env("PHEOBE_COPILOT_MODEL", "env-model");
    let ctx = WorkerCtx {
        model: Some("task-model".into()),
        ttl: Some(std::time::Duration::from_secs(120)),
        ..Default::default()
    };
    let out = CopilotWorker.run_with("the prompt", &dir, &ctx).unwrap();
    assert_eq!(out.final_text, "all good");
    assert_eq!(out.turns, Some(2));
    assert_eq!(out.tokens, Some(28716), "tokens come from the usage file");
    assert!(
        !usage_file(&dir).exists(),
        "the usage sidecar is removed after reading"
    );
    let captured = std::fs::read_to_string(&capture).unwrap();
    let lines: Vec<&str> = captured.lines().collect();
    assert_eq!(&lines[..2], ["-p", "the prompt"]);
    assert!(
        lines.windows(2).any(|w| w == ["-C", dir.to_str().unwrap()]),
        "{captured}"
    );
    assert!(
        lines.windows(2).any(|w| w == ["--model", "env-model"]),
        "env model wins: {captured}"
    );
    assert!(
        lines.contains(&"--allow-all-tools") && lines.contains(&"--disable-builtin-mcps"),
        "default flags: {captured}"
    );
    let cwd = lines.iter().find(|l| l.starts_with("CWD=")).unwrap();
    assert_eq!(
        std::fs::canonicalize(&cwd[4..]).unwrap(),
        std::fs::canonicalize(&dir).unwrap()
    );
}

#[test]
fn copilot_flags_env_replaces_the_default() {
    let _env = CopilotEnv::lock();
    let dir = scratch("flags");
    let (bin, capture) = capture_copilot(&dir, &stream("ok", 0), 0);
    set_env("PHEOBE_COPILOT_BIN", &bin);
    set_env("PHEOBE_COPILOT_FLAGS", "--allow-all");
    CopilotWorker.run("p", &dir).unwrap();
    let captured = std::fs::read_to_string(&capture).unwrap();
    assert!(captured.lines().any(|l| l == "--allow-all"), "{captured}");
    assert!(
        !captured.lines().any(|l| l == "--allow-all-tools"),
        "PHEOBE_COPILOT_FLAGS replaces the default: {captured}"
    );
    assert!(
        captured.lines().any(|l| l == "--deny-tool=shell(git push)"),
        "the push denial is policy, not a flag: {captured}"
    );
}

#[test]
fn copilot_failed_run_is_an_error() {
    let _env = CopilotEnv::lock();
    let dir = scratch("fail");
    let failing = format!(
        "{}\n{}",
        event(
            "session.error",
            serde_json::json!({"message": "No Copilot access"})
        ),
        serde_json::json!({"type": "result", "exitCode": 1})
    );
    let (bin, _) = capture_copilot(&dir, &failing, 1);
    set_env("PHEOBE_COPILOT_BIN", &bin);
    let err = CopilotWorker.run("p", &dir).unwrap_err().to_string();
    assert!(err.contains("No Copilot access"), "{err}");
}

#[test]
fn copilot_nonzero_exit_without_events_is_an_error() {
    let _env = CopilotEnv::lock();
    let dir = scratch("crash");
    let (bin, _) = capture_copilot(&dir, "not json", 3);
    set_env("PHEOBE_COPILOT_BIN", &bin);
    let err = CopilotWorker.run("p", &dir).unwrap_err().to_string();
    assert!(err.contains("exited with"), "{err}");
}

#[test]
fn copilot_strict_tier_is_refused() {
    let _env = CopilotEnv::lock();
    let dir = scratch("strict");
    let (bin, capture) = capture_copilot(&dir, &stream("x", 0), 0);
    set_env("PHEOBE_COPILOT_BIN", &bin);
    let ctx = WorkerCtx {
        sandbox: Some(crate::sandbox::Tier::Strict),
        ..Default::default()
    };
    let err = CopilotWorker
        .run_with("p", &dir, &ctx)
        .unwrap_err()
        .to_string();
    assert!(err.contains("strict"), "{err}");
    assert!(
        !capture.exists(),
        "copilot must not run under a refused tier"
    );
}
