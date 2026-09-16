use std::path::Path;

use super::worker_task;
use crate::agent::{run_worker, LoopCfg};
use crate::worker::Worker;

// ── opencode worker adapter (PHEOBE-4) — fake opencode on a temp PATH ───────
//
// The fake is a shell script that records its argv and cwd to files, then
// prints a canned `opencode run --format json` event stream. The stream
// mirrors the real 1.18.31 shapes verified from opencode source: a user
// side is implicit (the stream starts after submission), assistant text
// arrives as `text` parts and usage as `step_finish` parts (tokens +
// cost).
//
// env vars are process-global, so every env-touching test holds ENV_LOCK.

use crate::worker_opencode::OpenCodeWorker;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

fn env_root(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pheobe-opencode-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A fake `opencode` (or overridden bin name): records argv + cwd, emits
/// the canned stream. Returns the script path.
fn fake_bin(dir: &Path, name: &str, stream: &str) -> std::path::PathBuf {
    let argv_out = dir.join("argv.txt");
    let cwd_out = dir.join("cwd.txt");
    crate::tests::write_shim(
        dir,
        name,
        &format!(
            "printf '%s\\n' \"$@\" > {argv}\npwd > {cwd}\ncat <<'PHEOBE_STREAM_EOF'\n{stream}\nPHEOBE_STREAM_EOF\n",
            argv = argv_out.display(),
            cwd = cwd_out.display(),
        ),
    )
}

/// Canned stream: two assistant text parts (last wins) + two step_finish
/// usage parts. Expected: final_text = part2, tokens = 185 + 290 = 475,
/// usd = 0.15.
const CANNED_STREAM: &str = r#"{"type":"step_start","timestamp":1,"sessionID":"s1","part":{"id":"p0","type":"step-start"}}
{"type":"text","timestamp":2,"sessionID":"s1","part":{"id":"p1","type":"text","text":"assistant speaks first","time":{"start":1,"end":2}}}
{"type":"step_finish","timestamp":3,"sessionID":"s1","part":{"id":"p2","type":"step-finish","tokens":{"input":100,"output":50,"reasoning":10,"cache":{"read":20,"write":5}},"cost":0.05}}
{"type":"text","timestamp":4,"sessionID":"s1","part":{"id":"p3","type":"text","text":"assistant speaks last","time":{"start":3,"end":4}}}
{"type":"step_finish","timestamp":5,"sessionID":"s1","part":{"id":"p4","type":"step-finish","tokens":{"input":200,"output":60,"reasoning":0,"cache":{"read":30,"write":0}},"cost":0.1}}"#;

fn read_argv(bin_dir: &Path) -> Vec<String> {
    String::from_utf8_lossy(&std::fs::read(bin_dir.join("argv.txt")).unwrap())
        .lines()
        .map(|l| l.to_string())
        .collect()
}

fn opencodew(bin_path: &Path) -> OpenCodeWorker {
    unsafe { std::env::set_var("PHEOBE_OPENCODE_BIN", bin_path) };
    let w = OpenCodeWorker::from_env().unwrap();
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_BIN") };
    w
}

/// argv shape (mayfly precedent, verified vs `opencode run --help` 1.18.31):
/// `run --format json --auto --dir <worktree> <prompt>` — prompt last, as
/// one positional, and the child's cwd is the worktree.
#[test]
fn opencode_argv_shape_cwd_and_prompt_position() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = env_root("argv");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let w = opencodew(&fake_bin(&dir, "opencode", CANNED_STREAM));
    let out = w.run("the whole envelope", &wt).unwrap();
    assert_eq!(out.final_text, "assistant speaks last");

    let argv = read_argv(&dir);
    assert_eq!(argv[0], "run", "got: {argv:?}");
    assert!(
        argv.windows(2).any(|w| w == ["--format", "json"]),
        "got: {argv:?}"
    );
    assert!(argv.contains(&"--auto".to_string()), "got: {argv:?}");
    let dir_at = argv.iter().position(|a| a == "--dir").unwrap();
    assert_eq!(argv[dir_at + 1], wt.display().to_string(), "got: {argv:?}");
    assert_eq!(
        argv.last().unwrap(),
        "the whole envelope",
        "the prompt is the message positional (last arg)"
    );
    assert!(
        !argv.contains(&"-m".to_string()),
        "no -m without PHEOBE_OPENCODE_MODEL: {argv:?}"
    );
    assert!(
        !argv.contains(&"--attach".to_string()),
        "no --attach without PHEOBE_OPENCODE_URL: {argv:?}"
    );

    let cwd_raw = std::fs::read(dir.join("cwd.txt")).unwrap();
    let cwd = String::from_utf8_lossy(&cwd_raw);
    assert_eq!(
        cwd.trim(),
        wt.display().to_string(),
        "child cwd must be the worktree"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// final_text extracted (last text part wins) and usage summed across
/// step_finish events: 100+50+10+20+5 + 200+60+0+30+0 = 475, $0.15.
#[test]
fn opencode_final_text_and_usage_summed() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = env_root("parse");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let w = opencodew(&fake_bin(&dir, "opencode", CANNED_STREAM));
    let out = w.run("x", &wt).unwrap();
    assert_eq!(out.final_text, "assistant speaks last");
    assert_eq!(out.tokens, Some(475), "tokens = sum over step_finish parts");
    assert!(
        (out.usd.unwrap() - 0.15).abs() < 1e-9,
        "usd = sum over step_finish costs"
    );
    assert!(out.json_tail.is_none(), "prose is not handoff JSON");
    std::fs::remove_dir_all(&dir).ok();
}

/// End-to-end through run_worker (PHEOBE-9's guards): the prompt is the
/// protocol envelope (done_when + task_id present), and the engine's JSON
/// tail merges ONLY summary/next_steps/doubts into the handoff report.
#[test]
fn opencode_run_worker_envelope_prompt_and_json_tail_normalization() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = env_root("run-worker");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    // build the stream via serde_json so the JSON-as-text is escaped properly
    let tail = serde_json::json!({
        "ok": true, "summary": "did the thing", "next_steps": ["review the diff"],
        "doubts": ["assumed serde default"], "branch": "pheobe/sneaky"
    });
    let stream = serde_json::json!({
        "type": "text", "timestamp": 1, "sessionID": "s1",
        "part": {"id": "p1", "type": "text", "text": tail.to_string(), "time": {"start": 1, "end": 2}}
    })
    .to_string();
    let w = opencodew(&fake_bin(&dir, "opencode", &stream));

    let task = worker_task();
    let cfg = LoopCfg::default();
    let out = run_worker(&w, &task, &wt, "oc1", "", &[], &cfg).unwrap();
    assert!(out.ok);
    assert_eq!(out.summary.as_deref(), Some("did the thing"));
    assert_eq!(out.next_steps, vec!["review the diff"]);
    assert_eq!(out.doubts, vec!["assumed serde default"]);
    assert!(out.blocked.is_none());

    // the mechanical keys the engine claimed stay pheobe's
    let prompt_raw = std::fs::read(dir.join("argv.txt")).unwrap();
    let prompt = String::from_utf8_lossy(&prompt_raw);
    assert!(prompt.contains("done_when"), "envelope as prompt: {prompt}");
    assert!(
        prompt.contains("task_id=oc1"),
        "envelope as prompt: {prompt}"
    );
    assert!(
        !prompt.contains("pheobe/sneaky"),
        "branch stays mechanical, not prompted: {prompt}"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// Env overrides honored: PHEOBE_OPENCODE_BIN (different binary name),
/// PHEOBE_OPENCODE_MODEL → `-m`, PHEOBE_OPENCODE_URL → `--attach`.
#[test]
fn opencode_env_overrides_bin_model_and_attach() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = env_root("overrides");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let bin = fake_bin(&dir, "opencode-fancy", CANNED_STREAM);

    unsafe { std::env::set_var("PHEOBE_OPENCODE_BIN", &bin) };
    unsafe { std::env::set_var("PHEOBE_OPENCODE_MODEL", "opencode/big-pickle") };
    unsafe { std::env::set_var("PHEOBE_OPENCODE_URL", "http://127.0.0.1:4096") };
    let w = OpenCodeWorker::from_env().unwrap();
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_BIN") };
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_MODEL") };
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_URL") };

    let out = w.run("x", &wt).unwrap();
    assert_eq!(out.final_text, "assistant speaks last");
    let argv = read_argv(&dir);
    let m_at = argv.iter().position(|a| a == "-m").unwrap();
    assert_eq!(argv[m_at + 1], "opencode/big-pickle", "got: {argv:?}");
    let a_at = argv.iter().position(|a| a == "--attach").unwrap();
    assert_eq!(argv[a_at + 1], "http://127.0.0.1:4096", "got: {argv:?}");
    std::fs::remove_dir_all(&dir).ok();
}

/// Default bin name resolves through PATH: a fake `opencode` prepended on
/// the temp PATH wins over the real one (real PATH kept in the tail so
/// concurrent git subprocesses still resolve).
#[test]
fn opencode_default_bin_resolves_via_path() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = env_root("path");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    fake_bin(&dir, "opencode", CANNED_STREAM);
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_BIN") };
    let real_path = std::env::var("PATH").unwrap_or_default();
    unsafe { std::env::set_var("PATH", format!("{}:{}", dir.display(), real_path)) };
    let w = OpenCodeWorker::from_env().unwrap();
    let out = w.run("x", &wt).unwrap();
    unsafe { std::env::set_var("PATH", &real_path) };
    assert_eq!(out.final_text, "assistant speaks last");
    assert_eq!(read_argv(&dir)[0], "run");
    std::fs::remove_dir_all(&dir).ok();
}

/// PHEOBE_OPENCODE_TIMEOUT_SECS overrides the 600s default and the child
/// is actually killed (error, not a hang).
#[test]
fn opencode_timeout_override_kills_the_child() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = env_root("timeout");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let script = crate::tests::write_shim(&dir, "opencode", "sleep 30\n");
    unsafe { std::env::set_var("PHEOBE_OPENCODE_BIN", &script) };
    unsafe { std::env::set_var("PHEOBE_OPENCODE_TIMEOUT_SECS", "1") };
    let w = OpenCodeWorker::from_env().unwrap();
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_BIN") };
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_TIMEOUT_SECS") };

    let err = format!("{:#}", w.run("x", &wt).unwrap_err());
    assert!(err.contains("timeout of 1s"), "got: {err}");
    assert!(err.contains("PHEOBE_OPENCODE_TIMEOUT_SECS"), "got: {err}");
    std::fs::remove_dir_all(&dir).ok();
}

/// A missing binary produces a clear, actionable error.
#[test]
fn opencode_binary_not_found_error_is_clear() {
    let _g = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = env_root("notfound");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    unsafe { std::env::set_var("PHEOBE_OPENCODE_BIN", "no-such-opencode-binary-here") };
    let w = OpenCodeWorker::from_env().unwrap();
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_BIN") };

    let err = format!("{:#}", w.run("x", &wt).unwrap_err());
    assert!(
        err.contains("no-such-opencode-binary-here"),
        "names the culprit: {err}"
    );
    assert!(err.contains("not found"), "got: {err}");
    assert!(err.contains("PHEOBE_OPENCODE_BIN"), "names the fix: {err}");
    std::fs::remove_dir_all(&dir).ok();
}
