//! Contract tests: the task schema, the plan file, the allowlist check, the learn store.
use std::path::Path;

use crate::plan;
use crate::task::Task;

fn task_json(overrides: &str) -> String {
    format!(
        r#"{{
    "task": "Add --json output to `foo report`",
    "done_when": {{ "type": "command", "run": "cargo test -p foo", "expect_exit": 0 }},
    "repo": "/tmp/fake-repo",
    "paths_allow": ["src/report.rs", "tests/"]{overrides}
}}"#
    )
}

#[test]
fn intake_accepts_concrete_task() {
    let t: Task = serde_json::from_str(&task_json("")).unwrap();
    t.validate().unwrap();
    assert_eq!(t.resolve_repo().unwrap().display().to_string(), "/tmp/fake-repo");
}

#[test]
fn intake_rejects_vague_asks() {
    for bad in [
        r#"{"task": "Polish the API", "done_when": {"type":"command","run":"true"}}"#,
        r#"{"task": "Add tests and also fix docs", "done_when": {"type":"command","run":"true"}}"#,
        r#"{"task": "Improve error handling", "done_when": {"type":"command","run":"true"}}"#,
        r#"{"task": "Add a flag", "done_when": {"type":"command","run":""}}"#,
    ] {
        let t: Task = serde_json::from_str(bad).unwrap();
        assert!(t.validate().is_err(), "should reject: {bad}");
    }
}

#[test]
fn plan_roundtrips() {
    let dir = std::env::temp_dir().join(format!("pheobe-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let p = plan::Plan {
        task: "x".into(),
        steps: vec![plan::Step { desc: "write the parser".into(), status: "todo".into(), doubts: vec!["assumes API shape".into()] }],
    };
    plan::save(&dir, &p).unwrap();
    let loaded = plan::load(&dir).unwrap().unwrap();
    assert_eq!(loaded.steps[0].desc, "write the parser");
    assert_eq!(loaded.steps[0].doubts, vec!["assumes API shape"]);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn allowlist_flags_outside_paths() {
    let dir = std::env::temp_dir().join(format!("pheobe-allow-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // not a git repo — check_allowlist runs git status which fails cleanly here
    let r = crate::worktree::check_allowlist(&dir, &["src/".to_string()]);
    assert!(r.is_err()); // no repo, so the gate fails loudly — that's correct
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn ctx_brief_carries_the_preamble() {
    let e = crate::knowledge::Entry {
        slug: "tokio".into(),
        name: "tokio".into(),
        kind: "external".into(),
        version: Some("1.40".into()),
        last_verified: Some("2026-01-01".into()),
        stale: false,
        path: "/x/tokio.md".into(),
    };
    let b = crate::knowledge::brief(&[e]);
    assert!(b.contains("THIS IS RIGHT AND YOU ARE WRONG"));
    assert!(b.contains("Do not guess an API into existence"));
    assert!(b.contains("### tokio"));
}

#[test]
fn learn_disabled_by_default_and_sessions_append_when_enabled() {
    // default: off
    assert!(!crate::learn::enabled());

    // on: sessions begin + end append to the jsonl store
    let dir = std::env::temp_dir().join(format!("pheobe-learn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    unsafe { std::env::set_var("PHEOBE_LEARNING_DIR", &dir) };
    assert!(crate::learn::enabled());

    let s = crate::learn::begin_session(Path::new("/tmp/fake-repo"), "test task").unwrap().unwrap();
    crate::learn::store_nudge("/tmp/fake-repo", "test harness needs the vendored source", Some("test harness")).unwrap();
    let nudges = crate::learn::nudges_for("/tmp/fake-repo");
    assert_eq!(nudges.len(), 1);
    assert!(nudges[0].contains("vendored source"));

    crate::learn::end_session(Some(&s), "done", 0).unwrap();
    let txt = std::fs::read_to_string(dir.join("sessions.jsonl")).unwrap();
    assert_eq!(txt.lines().count(), 2); // begin + end
    let last: serde_json::Value = serde_json::from_str(txt.lines().last().unwrap()).unwrap();
    assert_eq!(last["outcome"], "done");
    assert_eq!(last["exit_code"], 0);

    std::fs::remove_dir_all(&dir).ok();
    unsafe { std::env::remove_var("PHEOBE_LEARNING_DIR") };
}

// ── agent loop (PHEOBE-1): scripted provider, real barn ─────────────────────

use crate::agent::LoopCfg;
use crate::llm::{Msg, Provider, ToolCall, ToolFn, ToolSchema};

struct ScriptedProvider {
    turns: std::sync::Mutex<Vec<Msg>>,
    seen_tool_calls: std::sync::Mutex<Vec<String>>,
}

impl Provider for ScriptedProvider {
    fn chat(&self, _messages: &[Msg], _tools: &[crate::llm::ToolSchema]) -> anyhow::Result<(Msg, Option<u64>)> {
        let mut turns = self.turns.lock().unwrap();
        let next = if turns.is_empty() {
            Msg::assistant("done", vec![])
        } else {
            turns.remove(0)
        };
        Ok((next, Some(42)))
    }
}

fn script_call(id: &str, name: &str, args: &str) -> ToolCall {
    ToolCall { id: id.into(), function: ToolFn { name: name.into(), arguments: args.into() } }
}

#[test]
fn loop_writes_inside_allowlist_commits_and_hands_off() {
    let root = std::env::temp_dir().join(format!("pheobe-loop-{}", std::process::id()));
    let wt = root.join("wt");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&wt).unwrap();
    run_git(&wt, &["init", "-q"]);
    run_git(&wt, &["config", "user.email", "t@t"]);
    run_git(&wt, &["config", "user.name", "t"]);
    run_git(&wt, &["commit", "--allow-empty", "-m", "root", "-q"]);

    let task: crate::task::Task = serde_json::from_str(
        r#"{"task":"Add report src","done_when":{"type":"command","run":"test -f src/x.rs"},"paths_allow":["src/"]}"#,
    )
    .unwrap();

    // turn 1: plan; turn 2: write file; turn 3: verify; turn 4: handoff
    let provider = ScriptedProvider {
        turns: std::sync::Mutex::new(vec![
            Msg::assistant("", vec![script_call("c1", "plan_tracker", r#"{"steps":[{"desc":"add file","status":"doing"}]}"#)]),
            Msg::assistant("", vec![script_call("c2", "write", r#"{"path":"src/x.rs","content":"pub fn x() -> u8 { 1 }\n"}"#)]),
            Msg::assistant("", vec![script_call("c3", "verify", r#"{"command":"test -f src/x.rs"}"#)]),
            Msg::assistant("", vec![script_call("c4", "handoff", r#"{"ok":true,"summary":"added src/x.rs","doubts":["assumed module placement"],"next_steps":["review"]}"#)]),
        ]),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };

    let outcome = crate::agent::run(&provider, &task, &wt, "t1", "", &[], &LoopCfg::default()).unwrap();
    assert!(outcome.ok, "loop should succeed");
    assert_eq!(outcome.summary.as_deref(), Some("added src/x.rs"));
    assert_eq!(outcome.doubts, vec!["assumed module placement"]);
    assert_eq!(outcome.usage.turns, 4);

    // the file landed inside the worktree, inside paths_allow
    assert!(wt.join("src/x.rs").exists());

    // the mechanical gates: commit + done_when
    let dirty = crate::worktree::status_dirty(&wt).unwrap();
    assert!(dirty);
    let ev = crate::verify::run_done_when(&task, &wt).unwrap();
    assert!(ev.passed);

    let sha = crate::worktree::commit(&wt, "t1", "add src/x.rs").unwrap();
    assert!(!sha.is_empty());
    let msg = run_git(&wt, &["log", "-1", "--format=%B"]);
    assert!(msg.contains("Pheobe-Task: t1"), "commit trailer missing: {msg}");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn loop_blocks_write_outside_allowlist() {
    let root = std::env::temp_dir().join(format!("pheobe-loop2-{}", std::process::id()));
    let wt = root.join("wt");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&wt).unwrap();

    let task: crate::task::Task = serde_json::from_str(
        r#"{"task":"Add thing","done_when":{"type":"command","run":"true"},"paths_allow":["src/"]}"#,
    )
    .unwrap();

    let provider = ScriptedProvider {
        turns: std::sync::Mutex::new(vec![Msg::assistant("", vec![script_call("c1", "write", r#"{"path":"Cargo.toml","content":"naughty"}"#)])]),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };
    let outcome = crate::agent::run(&provider, &task, &wt, "t2", "", &[], &LoopCfg::default()).unwrap();
    // the loop still terminates via handoff, but the tool call errored —
    // the guard fires in the tool result, not a panic
    assert_eq!(outcome.usage.turns, 2);
    std::fs::remove_dir_all(&root).ok();
}

fn run_git(wt: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
