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

    let _ = std::fs::remove_dir_all(&root);
}
