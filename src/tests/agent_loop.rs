//! Agent loop (PHEOBE-1): scripted provider against the real barn —
//! writes inside the allowlist commit + hand off; writes outside are
//! refused in the tool result, never a panic.

use super::run_git;
use super::scripted::{script_call, ScriptedProvider};
use crate::agent::LoopCfg;
use crate::llm::Msg;

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
            Msg::assistant(
                "",
                vec![script_call(
                    "c1",
                    "plan_tracker",
                    r#"{"steps":[{"desc":"add file","status":"doing"}]}"#,
                )],
            ),
            Msg::assistant(
                "",
                vec![script_call(
                    "c2",
                    "write",
                    r#"{"path":"src/x.rs","content":"pub fn x() -> u8 { 1 }\n"}"#,
                )],
            ),
            Msg::assistant(
                "",
                vec![script_call(
                    "c3",
                    "verify",
                    r#"{"command":"test -f src/x.rs"}"#,
                )],
            ),
            Msg::assistant(
                "",
                vec![script_call(
                    "c4",
                    "handoff",
                    r#"{"ok":true,"summary":"added src/x.rs","doubts":["assumed module placement"],"next_steps":["review"]}"#,
                )],
            ),
        ]),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };

    let outcome =
        crate::agent::run(&provider, &task, &wt, "t1", "", &[], &LoopCfg::default()).unwrap();
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

    let sha = crate::worktree::commit(&wt, "t1", "add src/x.rs", &["src/".to_string()])
        .unwrap()
        .expect("something to commit");
    assert!(!sha.is_empty());
    let msg = run_git(&wt, &["log", "-1", "--format=%B"]);
    assert!(
        msg.contains("Pheobe-Task: t1"),
        "commit trailer missing: {msg}"
    );

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
        turns: std::sync::Mutex::new(vec![Msg::assistant(
            "",
            vec![script_call(
                "c1",
                "write",
                r#"{"path":"Cargo.toml","content":"naughty"}"#,
            )],
        )]),
        seen_tool_calls: std::sync::Mutex::new(vec![]),
    };
    let outcome =
        crate::agent::run(&provider, &task, &wt, "t2", "", &[], &LoopCfg::default()).unwrap();
    // the loop still terminates via handoff, but the tool call errored —
    // the guard fires in the tool result, not a panic
    assert_eq!(outcome.usage.turns, 2);
    std::fs::remove_dir_all(&root).ok();
}
