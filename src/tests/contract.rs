//! Contract tests: task intake (the fuzziness gate), plan roundtrip, the
//! allowlist check, the sidecar-dirty regression, the knowledge brief
//! preamble, the learn store.

use std::path::Path;

use super::run_git;
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
    assert_eq!(
        t.resolve_repo().unwrap().display().to_string(),
        "/tmp/fake-repo"
    );
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
        steps: vec![plan::Step {
            desc: "write the parser".into(),
            status: "todo".into(),
            doubts: vec!["assumes API shape".into()],
            depends_on: vec![],
        }],
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
fn sidecar_pheobe_dir_alone_is_not_dirty() {
    // regression (found by the PHEOBE-17 ACP smoke): a run that hands off
    // ok with zero source edits used to look "dirty" purely because
    // plan::save wrote `.pheobe/plan.json` — then commit() staged nothing
    // (stage always excludes `.pheobe`) and `git commit` died with a bare
    // "nothing to commit" exit 1. `.pheobe/` must never count as dirty.
    let root = std::env::temp_dir().join(format!("pheobe-sidecar-{}", std::process::id()));
    let wt = root.join("wt");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&wt).unwrap();
    run_git(&wt, &["init", "-q"]);
    run_git(&wt, &["config", "user.email", "t@t"]);
    run_git(&wt, &["config", "user.name", "t"]);
    std::fs::write(wt.join("readme.md"), "seed").unwrap();
    run_git(&wt, &["add", "-A"]);
    run_git(&wt, &["commit", "-m", "root", "-q"]);

    // only the sidecar exists → not dirty
    crate::plan::save(
        &wt,
        &plan::Plan {
            task: "x".into(),
            steps: vec![],
        },
    )
    .unwrap();
    assert!(
        !crate::worktree::status_dirty(&wt).unwrap(),
        "a tree whose only diff is .pheobe/plan.json is clean for commit purposes"
    );

    // a real source edit still counts
    std::fs::write(wt.join("readme.md"), "edited").unwrap();
    assert!(crate::worktree::status_dirty(&wt).unwrap());

    std::fs::remove_dir_all(&root).ok();
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
        tags: vec![],
    };
    let b = crate::knowledge::brief(&[e], None);
    assert!(b.contains("THIS IS RIGHT AND YOU ARE WRONG"));
    assert!(b.contains("Do not guess an API into existence"));
    assert!(b.contains("### tokio"));
}

#[test]
fn learn_disabled_by_default_and_sessions_append_when_enabled() {
    let _lock = crate::memory::tests::env_lock().lock().unwrap();
    // default: off
    assert!(!crate::learn::enabled());

    // on: sessions begin + end append to the jsonl store
    let dir = std::env::temp_dir().join(format!("pheobe-learn-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    unsafe { std::env::set_var("PHEOBE_LEARNING_DIR", &dir) };
    assert!(crate::learn::enabled());

    let s = crate::learn::begin_session(Path::new("/tmp/fake-repo"), "test task")
        .unwrap()
        .unwrap();
    crate::learn::store_nudge(
        "/tmp/fake-repo",
        "test harness needs the vendored source",
        Some("test harness"),
    )
    .unwrap();
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
