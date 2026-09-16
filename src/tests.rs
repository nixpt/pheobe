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
        steps: vec![plan::Step { desc: "write the parser".into(), status: "todo".into(), doubts: vec!["assumes API shape".into()], depends_on: vec![] }],
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
use crate::llm::{Msg, Provider, ToolCall, ToolFn};

struct ScriptedProvider {
    turns: std::sync::Mutex<Vec<Msg>>,
    #[allow(dead_code)] // written per-turn below; read only by tests that want it
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

    let sha = crate::worktree::commit(&wt, "t1", "add src/x.rs", &["src/".to_string()]).unwrap();
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

// ── aging ladder + budgets (PHEOBE-2) ───────────────────────────────────────

use crate::aging::{parse_ttl, Ladder, State};
use std::time::Duration;

#[test]
fn ttl_parses_mayfly_style_durations() {
    assert_eq!(parse_ttl("45m").unwrap(), Duration::from_secs(2700));
    assert_eq!(parse_ttl("2h").unwrap(), Duration::from_secs(7200));
    assert_eq!(parse_ttl("90s").unwrap(), Duration::from_secs(90));
    assert_eq!(parse_ttl("1d").unwrap(), Duration::from_secs(86400));
    assert_eq!(parse_ttl("0s").unwrap(), Duration::ZERO);
    assert!(parse_ttl("45x").is_err());
    assert!(parse_ttl("nope").is_err());
}

#[test]
fn ladder_states_match_the_mayfly_table() {
    let ladder = Ladder::new(Some(Duration::from_secs(100)));
    assert_eq!(ladder.state_at(Duration::from_secs(10)), State::Alive);
    assert_eq!(ladder.state_at(Duration::from_secs(55)), State::Warn);
    assert_eq!(ladder.state_at(Duration::from_secs(80)), State::Narrow);
    assert_eq!(ladder.state_at(Duration::from_secs(100)), State::Expired);
    // no ttl = eternal worker (the loop's other guards still apply)
    assert_eq!(Ladder::new(None).state_at(Duration::from_secs(9999)), State::Alive);
    // injectables exist for the live transitions only
    assert!(Ladder::inject(State::Warn).unwrap().contains("Narrow"));
    assert!(Ladder::inject(State::Narrow).unwrap().contains("final push"));
    assert!(Ladder::inject(State::Expired).is_none());
}

#[test]
fn loop_hard_stops_on_expired_ttl() {
    let root = std::env::temp_dir().join(format!("pheobe-ttl-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let task: Task = serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#).unwrap();
    let provider = ScriptedProvider {
        // endless turns — the ladder must cut them off
        turns: std::sync::Mutex::new((0..50).map(|i| Msg::assistant("", vec![script_call(&format!("c{i}"), "bash", r#"{"command":"true"}"#)])).collect()),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };
    let cfg = LoopCfg { ttl: Some(Duration::ZERO), ..Default::default() };
    let out = crate::agent::run(&provider, &task, &root, "ttl", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("ttl_exceeded"), "got: {blocked}");
    assert!(blocked.contains("task-design failure"));
    assert_eq!(out.usage.turns, 0); // cut off before any turn ran
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn loop_stops_on_usd_budget() {
    let root = std::env::temp_dir().join(format!("pheobe-usd-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let task: Task = serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#).unwrap();
    let provider = ScriptedProvider {
        turns: std::sync::Mutex::new((0..50).map(|i| Msg::assistant("", vec![script_call(&format!("c{i}"), "bash", r#"{"command":"true"}"#)])).collect()),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };
    // mock reports 42 tokens/turn; rate $2500/Mtok → $0.105/turn → capped ~turn 20
    let cfg = LoopCfg { max_usd: Some(2.0), usd_per_mtok: Some(2500.0), ..Default::default() };
    let out = crate::agent::run(&provider, &task, &root, "usd", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("budget_exceeded"), "got: {blocked}");
    assert!(out.usage.turns <= 25, "budget must cut the run early, got {} turns", out.usage.turns);
    std::fs::remove_dir_all(&root).ok();
}

// ── .jagent/issues regression tests (verified live on Zen, s456) ────────────

fn git_repo_with(name: &str, content: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("pheobe-{}-{}-{}", name, std::process::id(), n));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    crate::tests::run_git(&root, &["init", "-q"]);
    crate::tests::run_git(&root, &["config", "user.email", "t@t"]);
    crate::tests::run_git(&root, &["config", "user.name", "t"]);
    std::fs::write(root.join(name), content).unwrap();
    crate::tests::run_git(&root, &["add", "."]);
    crate::tests::run_git(&root, &["commit", "-q", "-m", "init"]);
    root
}

/// Issue 01: single modified file as the FIRST porcelain line — ` M calc.py`
/// must parse to `calc.py`, not `alc.py`.
#[test]
fn issue01_first_porcelain_line_not_mangled() {
    let root = git_repo_with("calc.py", "def add(a,b):\n    return a-b\n");
    std::fs::write(root.join("calc.py"), "def add(a,b):\n    return a+b\n").unwrap();

    let (violations, _by) = crate::worktree::check_allowlist(&root, &["calc.py".to_string()]).unwrap();
    assert!(violations.is_empty(), "edited allowlisted file must pass, got: {violations:?}");

    // a genuinely outside edit (tracked file, e.g. via bash sed) is caught
    // with its FULL name; untracked junk would be a byproduct instead
    let other = git_repo_with("calc.py", "x");
    std::fs::write(other.join("other.py"), "z").unwrap();
    crate::tests::run_git(&other, &["add", "."]);
    crate::tests::run_git(&other, &["commit", "-q", "-m", "add other"]);
    std::fs::write(other.join("other.py"), "w").unwrap();
    let (violations, _) = crate::worktree::check_allowlist(&other, &["calc.py".to_string()]).unwrap();
    assert_eq!(violations, vec!["other.py".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// Issue 02: pheobe's own state dir is never a violation; bash-run byproducts
/// are reported separately and staged OUT of the commit.
#[test]
fn issue02_state_dir_exempt_and_byproducts_staged_out() {
    let root = git_repo_with("calc.py", "def add(a,b):\n    return a-b\n");
    std::fs::write(root.join("calc.py"), "def add(a,b):\n    return a+b\n").unwrap();
    std::fs::create_dir_all(root.join(".pheobe")).unwrap();
    std::fs::write(root.join(".pheobe/plan.json"), "{}").unwrap();
    std::fs::create_dir_all(root.join("__pycache__")).unwrap();
    std::fs::write(root.join("__pycache__/junk.py"), "x").unwrap();

    let (violations, byproducts) = crate::worktree::check_allowlist(&root, &["calc.py".to_string()]).unwrap();
    assert!(violations.is_empty(), ".pheobe must be exempt, got: {violations:?}");
    assert_eq!(byproducts, vec!["__pycache__/".to_string()]);

    // commit stages ONLY the allowlisted file — the byproduct stays out
    let sha = crate::worktree::commit(&root, "t", "fix add", &["calc.py".to_string()]).unwrap();
    assert!(!sha.is_empty());
    let committed = crate::tests::run_git(&root, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(committed.contains("calc.py"), "got: {committed}");
    assert!(!committed.contains("__pycache__"), "byproduct staged in: {committed}");
    assert!(!committed.contains(".pheobe"), "state staged in: {committed}");
    let _ = std::fs::remove_dir_all(&root);
}

/// Issue 03: an existing `pheobe/<slug>` branch never gets silently reused.
#[test]
fn issue03_existing_branch_suffixed_not_reused() {
    let root = git_repo_with("f.txt", "a\n");
    crate::tests::run_git(&root, &["branch", "pheobe/demo"]);
    let b = crate::worktree::next_free_branch(&root, "pheobe/demo").unwrap();
    assert_eq!(b, "pheobe/demo-2");
    crate::tests::run_git(&root, &["branch", "pheobe/demo-2"]);
    let b2 = crate::worktree::next_free_branch(&root, "pheobe/demo").unwrap();
    assert_eq!(b2, "pheobe/demo-3");
    // free base comes back untouched
    let b3 = crate::worktree::next_free_branch(&root, "pheobe/fresh").unwrap();
    assert_eq!(b3, "pheobe/fresh");
    let _ = std::fs::remove_dir_all(&root);
}

/// Loops/graphs research: plan steps gain optional `depends_on` edges —
/// a parent that fanned out several pheobe nodes can consume this plan as
/// a sub-DAG. Validation lives in the plan_tracker tool, not an engine.
#[test]
fn plan_steps_carry_dependency_edges() {
    let steps: Vec<plan::Step> = serde_json::from_str(
        r#"[
        {"desc": "add module", "status": "done"},
        {"desc": "wire CLI flag", "status": "todo", "depends_on": [0]},
        {"desc": "tests", "status": "todo", "depends_on": [0, 1]}
    ]"#,
    )
    .unwrap();
    assert!(steps[0].depends_on.is_empty());
    assert_eq!(steps[1].depends_on, vec![0]);
    assert_eq!(steps[2].depends_on, vec![0, 1]);
}

// ── Worker trait + dispatcher (PHEOBE-9) ────────────────────────────────────

use crate::agent::run_worker;
use crate::worker::{worker_from_env, Worker, WorkerOutcome};

struct ScriptedWorker {
    calls: std::sync::Mutex<Vec<String>>,
    outcome: WorkerOutcome,
}

impl Worker for ScriptedWorker {
    fn run(&self, prompt: &str, _worktree: &Path) -> anyhow::Result<WorkerOutcome> {
        self.calls.lock().unwrap().push(prompt.to_string());
        Ok(WorkerOutcome {
            final_text: self.outcome.final_text.clone(),
            tokens: self.outcome.tokens,
            usd: self.outcome.usd,
            json_tail: self.outcome.json_tail.clone(),
        })
    }
}

fn worker_task() -> Task {
    serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#).unwrap()
}

fn worker_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("pheobe-worker-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// (a) worker path with a prose tail: summary comes from final_text, the
/// worker gets the same prompt the loop builds, and the mechanical gates
/// downstream see usage.
#[test]
fn worker_prose_tail_becomes_summary() {
    let dir = worker_dir("prose");
    let task = worker_task();
    let w = ScriptedWorker {
        calls: std::sync::Mutex::new(vec![]),
        outcome: WorkerOutcome {
            final_text: "wrote the parser and wired the flag".into(),
            tokens: Some(1200),
            usd: None,
            json_tail: None,
        },
    };
    let out = run_worker(&w, &task, &dir, "w1", "", &[], &LoopCfg::default()).unwrap();
    assert!(out.ok);
    assert_eq!(out.summary.as_deref(), Some("wrote the parser and wired the flag"));
    assert!(out.next_steps.is_empty());
    assert!(out.doubts.is_empty());
    assert!(out.blocked.is_none());
    assert_eq!(out.usage.turns, 1);
    assert_eq!(out.usage.total_tokens, Some(1200));

    let calls = w.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "one prompt out, one result back");
    assert!(calls[0].contains("done_when"), "worker prompt must be the shared loop prompt");
    assert!(calls[0].contains("task_id=w1"), "got: {}", calls[0]);
    std::fs::remove_dir_all(&dir).ok();
}

/// (b) worker path with a JSON tail: only summary/next_steps/doubts merge
/// from it; engine claims about branch/commits/tests are ignored (those
/// stay mechanical, filled in cmd_run).
#[test]
fn worker_json_tail_merges_report_contract_only() {
    let dir = worker_dir("json");
    let task = worker_task();
    let w = ScriptedWorker {
        calls: std::sync::Mutex::new(vec![]),
        outcome: WorkerOutcome {
            final_text: "prose fallback".into(),
            tokens: Some(500),
            usd: None,
            json_tail: Some(serde_json::json!({
                "ok": true,
                "summary": "did the thing",
                "next_steps": ["review the diff"],
                "doubts": ["assumed serde default"],
                "branch": "pheobe/sneaky",
                "commits": ["deadbeef"],
                "tests": {"ran": "cargo test", "passed": true}
            })),
        },
    };
    let out = run_worker(&w, &task, &dir, "w2", "", &[], &LoopCfg::default()).unwrap();
    assert!(out.ok);
    assert_eq!(out.summary.as_deref(), Some("did the thing"), "json summary wins over prose");
    assert_eq!(out.next_steps, vec!["review the diff"]);
    assert_eq!(out.doubts, vec!["assumed serde default"]);
    assert!(out.blocked.is_none());

    // a JSON tail without a summary key leaves the prose as the summary
    let w2 = ScriptedWorker {
        calls: std::sync::Mutex::new(vec![]),
        outcome: WorkerOutcome {
            final_text: "prose still counts".into(),
            tokens: None,
            usd: None,
            json_tail: Some(serde_json::json!({"doubts": ["only doubts"]})),
        },
    };
    let out2 = run_worker(&w2, &task, &dir, "w2b", "", &[], &LoopCfg::default()).unwrap();
    assert_eq!(out2.summary.as_deref(), Some("prose still counts"));
    assert_eq!(out2.doubts, vec!["only doubts"]);
    std::fs::remove_dir_all(&dir).ok();
}

/// (c) the aging ladder hard-stops AROUND a worker call: ttl=0s means
/// Expired before the call — the worker is never invoked.
#[test]
fn worker_ttl_hard_stop_before_the_call() {
    let dir = worker_dir("ttl-pre");
    let task = worker_task();
    let w = ScriptedWorker {
        calls: std::sync::Mutex::new(vec![]),
        outcome: WorkerOutcome {
            final_text: "never happened".into(),
            tokens: Some(1),
            usd: None,
            json_tail: None,
        },
    };
    let cfg = LoopCfg { ttl: Some(Duration::ZERO), ..Default::default() };
    let out = run_worker(&w, &task, &dir, "ttl-pre", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("ttl_exceeded"), "got: {blocked}");
    assert!(blocked.contains("task-design failure"), "got: {blocked}");
    assert!(w.calls.lock().unwrap().is_empty(), "worker must NOT be called once expired");
    assert_eq!(out.usage.turns, 0);
    std::fs::remove_dir_all(&dir).ok();
}

/// (c continued) the AFTER-side guard: an engine whose whole run lands past
/// the deadline gets its result ignored — blocked, not ok. A 20ms ttl with a
/// 60ms sleep is deterministic: elapsed ≥ 60ms > 20ms always.
#[test]
fn worker_result_ignored_when_ttl_expires_during_the_call() {
    struct SlowWorker;
    impl Worker for SlowWorker {
        fn run(&self, _prompt: &str, _worktree: &Path) -> anyhow::Result<WorkerOutcome> {
            std::thread::sleep(Duration::from_millis(60));
            Ok(WorkerOutcome {
                final_text: "engine thinks it finished".into(),
                tokens: Some(10),
                usd: None,
                json_tail: None,
            })
        }
    }
    let dir = worker_dir("ttl-post");
    let task = worker_task();
    let cfg = LoopCfg { ttl: Some(Duration::from_millis(20)), ..Default::default() };
    let out = run_worker(&SlowWorker, &task, &dir, "ttl-post", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("ttl_exceeded"), "got: {blocked}");
    assert_eq!(out.summary.as_deref(), Some(""), "result must be ignored, not reported");
    std::fs::remove_dir_all(&dir).ok();
}

/// (d) the registry: 'openai' resolves to the built-in loop (None); an
/// unknown PHEOBE_PROVIDER errors clearly, naming the culprit.
#[test]
fn provider_registry_resolves_or_errors_clearly() {
    assert!(matches!(worker_from_env("openai"), Ok(None)), "openai = the built-in loop");
    assert!(matches!(worker_from_env(""), Ok(None)), "empty = the default loop");
    let err = match worker_from_env("claude") {
        Err(e) => e,
        Ok(_) => panic!("unknown provider must error"),
    };
    let msg = format!("{err:#}");
    assert!(msg.contains("unknown PHEOBE_PROVIDER 'claude'"), "got: {msg}");
    assert!(msg.contains("no worker adapter registered"), "got: {msg}");
}

// ── PHEOBE-10: plan_tracker depends_on validation ───────────────────────────

fn p10_step(desc: &str, status: &str, deps: Vec<u32>) -> plan::Step {
    plan::Step { desc: desc.into(), status: status.into(), doubts: vec![], depends_on: deps }
}

#[test]
fn p10_plan_depends_on_validation() {
    use plan::validate_steps;

    // valid DAG passes
    assert!(validate_steps(&[
        p10_step("a", "done", vec![]),
        p10_step("b", "done", vec![0]),
        p10_step("c", "todo", vec![0, 1]),
    ])
    .is_ok());

    // out-of-range index
    let e = validate_steps(&[p10_step("a", "todo", vec![5])]).unwrap_err();
    assert!(e.contains("out of range"), "{e}");

    // self-reference
    let e = validate_steps(&[p10_step("a", "todo", vec![0])]).unwrap_err();
    assert!(e.contains("depends on itself"), "{e}");

    // cycle surfaces as cycle_detected with the path
    let e = validate_steps(&[
        p10_step("a", "todo", vec![1]),
        p10_step("b", "todo", vec![2]),
        p10_step("c", "todo", vec![0]),
    ])
    .unwrap_err();
    assert!(e.contains("cycle_detected"), "{e}");

    // done step with an undone predecessor is refused; blockers are listed
    let e = validate_steps(&[p10_step("a", "doing", vec![]), p10_step("b", "done", vec![0])])
        .unwrap_err();
    assert!(e.contains("cannot be marked done"), "{e}");
    assert!(e.contains("blocking predecessors"), "{e}");
    assert!(e.contains("'a'"), "blocker desc in refusal: {e}");
}

// ── PHEOBE-10: checkpoints ───────────────────────────────────────────────────

#[test]
fn p10_checkpoint_create_restore_roundtrip() {
    use crate::checkpoint as cp;
    let root = git_repo_with("f.txt", "one\n");

    // clean tree → recorded, no stash to restore
    cp::create(&root, "clean-point").unwrap();
    let l = cp::list(&root).unwrap();
    assert_eq!(l.len(), 1);
    assert!(l[0].stash.is_none());
    assert!(cp::restore(&root, "clean-point").is_err());

    // dirty state at checkpoint time
    std::fs::write(root.join("f.txt"), "two\n").unwrap();
    let msg = cp::create(&root, "before").unwrap();
    assert!(msg.contains("before") && msg.contains("stash="), "{msg}");

    // the failed-iteration shape: edit backwards, then restore brings the
    // checkpoint state back
    std::fs::write(root.join("f.txt"), "one\n").unwrap();
    cp::restore(&root, "before").unwrap();
    assert_eq!(std::fs::read_to_string(root.join("f.txt")).unwrap(), "two\n");

    // prune keeps the newest 5 (upsert'd names stay distinct)
    for i in 0..7 {
        cp::create(&root, &format!("bulk-{i}")).unwrap();
    }
    let msg = cp::prune(&root, 5).unwrap();
    assert!(msg.contains("pruned 4"), "{msg}");
    let l = cp::list(&root).unwrap();
    assert_eq!(l.len(), 5);
    assert!(l.iter().any(|c| c.name == "bulk-6"));
    assert!(!l.iter().any(|c| c.name == "clean-point"));

    // bad names refused
    assert!(cp::create(&root, "bad name!").is_err());

    let _ = std::fs::remove_dir_all(&root);}

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
    let script = dir.join(name);
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {argv}\npwd > {cwd}\ncat <<'PHEOBE_STREAM_EOF'\n{stream}\nPHEOBE_STREAM_EOF\n",
            argv = argv_out.display(),
            cwd = cwd_out.display(),
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    script
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
    let _g = ENV_LOCK.lock().unwrap();
    let dir = env_root("argv");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let w = opencodew(&fake_bin(&dir, "opencode", CANNED_STREAM));
    let out = w.run("the whole envelope", &wt).unwrap();
    assert_eq!(out.final_text, "assistant speaks last");

    let argv = read_argv(&dir);
    assert_eq!(argv[0], "run", "got: {argv:?}");
    assert!(argv.windows(2).any(|w| w == ["--format", "json"]), "got: {argv:?}");
    assert!(argv.contains(&"--auto".to_string()), "got: {argv:?}");
    let dir_at = argv.iter().position(|a| a == "--dir").unwrap();
    assert_eq!(argv[dir_at + 1], wt.display().to_string(), "got: {argv:?}");
    assert_eq!(
        argv.last().unwrap(),
        "the whole envelope",
        "the prompt is the message positional (last arg)"
    );
    assert!(!argv.contains(&"-m".to_string()), "no -m without PHEOBE_OPENCODE_MODEL: {argv:?}");
    assert!(!argv.contains(&"--attach".to_string()), "no --attach without PHEOBE_OPENCODE_URL: {argv:?}");

    let cwd_raw = std::fs::read(dir.join("cwd.txt")).unwrap();
    let cwd = String::from_utf8_lossy(&cwd_raw);
    assert_eq!(cwd.trim(), wt.display().to_string(), "child cwd must be the worktree");
    std::fs::remove_dir_all(&dir).ok();
}

/// final_text extracted (last text part wins) and usage summed across
/// step_finish events: 100+50+10+20+5 + 200+60+0+30+0 = 475, $0.15.
#[test]
fn opencode_final_text_and_usage_summed() {
    let _g = ENV_LOCK.lock().unwrap();
    let dir = env_root("parse");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let w = opencodew(&fake_bin(&dir, "opencode", CANNED_STREAM));
    let out = w.run("x", &wt).unwrap();
    assert_eq!(out.final_text, "assistant speaks last");
    assert_eq!(out.tokens, Some(475), "tokens = sum over step_finish parts");
    assert!((out.usd.unwrap() - 0.15).abs() < 1e-9, "usd = sum over step_finish costs");
    assert!(out.json_tail.is_none(), "prose is not handoff JSON");
    std::fs::remove_dir_all(&dir).ok();
}

/// End-to-end through run_worker (PHEOBE-9's guards): the prompt is the
/// protocol envelope (done_when + task_id present), and the engine's JSON
/// tail merges ONLY summary/next_steps/doubts into the handoff report.
#[test]
fn opencode_run_worker_envelope_prompt_and_json_tail_normalization() {
    let _g = ENV_LOCK.lock().unwrap();
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
    assert!(prompt.contains("task_id=oc1"), "envelope as prompt: {prompt}");
    assert!(!prompt.contains("pheobe/sneaky"), "branch stays mechanical, not prompted: {prompt}");
    std::fs::remove_dir_all(&dir).ok();
}

/// Env overrides honored: PHEOBE_OPENCODE_BIN (different binary name),
/// PHEOBE_OPENCODE_MODEL → `-m`, PHEOBE_OPENCODE_URL → `--attach`.
#[test]
fn opencode_env_overrides_bin_model_and_attach() {
    let _g = ENV_LOCK.lock().unwrap();
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
    let _g = ENV_LOCK.lock().unwrap();
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
    let _g = ENV_LOCK.lock().unwrap();
    let dir = env_root("timeout");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    let script = dir.join("opencode");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
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
    let _g = ENV_LOCK.lock().unwrap();
    let dir = env_root("notfound");
    let wt = dir.join("wt");
    std::fs::create_dir_all(&wt).unwrap();
    unsafe { std::env::set_var("PHEOBE_OPENCODE_BIN", "no-such-opencode-binary-here") };
    let w = OpenCodeWorker::from_env().unwrap();
    unsafe { std::env::remove_var("PHEOBE_OPENCODE_BIN") };

    let err = format!("{:#}", w.run("x", &wt).unwrap_err());
    assert!(err.contains("no-such-opencode-binary-here"), "names the culprit: {err}");
    assert!(err.contains("not found"), "got: {err}");
    assert!(err.contains("PHEOBE_OPENCODE_BIN"), "names the fix: {err}");
    std::fs::remove_dir_all(&dir).ok();
}
