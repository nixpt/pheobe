//! Contract tests: the task schema, the plan file, the allowlist check.

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
