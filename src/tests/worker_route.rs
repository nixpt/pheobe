//! Worker-route report fidelity (s457 adoption test, issues 05/06/07): the
//! engine's own commits reach the report, a fenced handoff JSON is merged,
//! and the worker prompt names no self-loop tools.
use super::{git_repo_with, run_git};
use crate::agent::{build_prompt_for, PromptMode};
use crate::worker::extract_json_tail;

// The exact tail Claude Code emitted on 2026-09-16 (two sentences, then a
// ```json fence) — issue 06's reproduction.
const CLAUDE_TAIL: &str = "No remote is configured on this repo, so the push step has nowhere to go; the commit is on the branch for the parent to merge.\n\nThe `handoff` tool isn't exposed in this session, so the report follows the contract verbatim:\n\n```json\n{\n  \"ok\": true,\n  \"summary\": \"calc.pow(a, b) returns a ** b. Committed as f16680b.\",\n  \"next_steps\": [\"Merge pheobe/worker-claude-pow into main.\"],\n  \"doubts\": [\"Name pow shadows the builtin inside calc.py.\"],\n  \"blocked\": \"\"\n}\n```";

#[test]
fn fenced_report_with_prose_around_it_is_the_json_tail() {
    let v = extract_json_tail(CLAUDE_TAIL).expect("fenced contract parses");
    assert_eq!(v["ok"], true);
    assert!(v["summary"].as_str().unwrap().starts_with("calc.pow"));
    assert_eq!(v["next_steps"].as_array().unwrap().len(), 1);
    assert_eq!(v["doubts"].as_array().unwrap().len(), 1);
    assert!(v.get("blocked").is_none(), "empty blocked is a non-block");
}

#[test]
fn bare_and_embedded_reports_parse_and_prose_does_not() {
    let bare = r#"{"ok": false, "summary": "x", "blocked": "no python"}"#;
    assert_eq!(extract_json_tail(bare).unwrap()["blocked"], "no python");
    let embedded = "Done. Report: {\"ok\": true, \"summary\": \"y\"} — bye";
    assert_eq!(extract_json_tail(embedded).unwrap()["summary"], "y");
    assert!(extract_json_tail("all good, nothing to report").is_none());
    // an object that is not handoff-shaped is prose too (a config dump, say)
    assert!(extract_json_tail(r#"{"model": "x", "tokens": 3}"#).is_none());
}

#[test]
fn last_fence_wins_when_the_engine_shows_its_work() {
    let text = "First I planned:\n```json\n{\"steps\": []}\n```\nthen finished:\n```json\n{\"ok\": true, \"summary\": \"final\"}\n```";
    assert_eq!(extract_json_tail(text).unwrap()["summary"], "final");
}

#[test]
fn commits_since_base_lists_engine_made_commits() {
    let repo = git_repo_with("a.txt", "one\n");
    let base = crate::worktree::head_sha(&repo).unwrap();
    assert!(crate::worktree::commits_since(&repo, &base)
        .unwrap()
        .is_empty());
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    run_git(&repo, &["commit", "-qam", "engine: first"]);
    std::fs::write(repo.join("a.txt"), "three\n").unwrap();
    run_git(&repo, &["commit", "-qam", "engine: second"]);
    let shas = crate::worktree::commits_since(&repo, &base).unwrap();
    assert_eq!(shas.len(), 2, "both engine commits, oldest first: {shas:?}");
    assert_eq!(shas[1], run_git(&repo, &["rev-parse", "--short", "HEAD"]));
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn worker_prompt_names_no_self_loop_tools() {
    let task: crate::task::Task =
        serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#)
            .unwrap();
    let w = build_prompt_for(PromptMode::Worker, &task, "t1", "", &[]);
    let s = build_prompt_for(PromptMode::SelfLoop, &task, "t1", "", &[]);
    for tool in ["plan_tracker", "calling handoff", "the verify tool"] {
        assert!(s.contains(tool), "self loop keeps `{tool}`");
        assert!(!w.contains(tool), "worker prompt must not mention `{tool}`");
    }
    assert!(w.contains(".pheobe/plan.json"));
    assert!(w.contains("Pheobe-Task: <task_id>"));
    assert!(w.contains("There is no handoff tool here"));
    assert!(w.contains("FINAL message"));
}

#[test]
fn commit_gate_is_a_no_op_when_only_byproducts_are_dirty() {
    // issue 09: the engine committed its own work and left __pycache__/
    // behind; the tree is "dirty" but nothing is stageable.
    let repo = git_repo_with("calc.py", "x = 1\n");
    std::fs::create_dir_all(repo.join("__pycache__")).unwrap();
    std::fs::write(repo.join("__pycache__/calc.pyc"), b"\0").unwrap();
    let r = crate::worktree::commit(&repo, "t9", "gate", &["calc.py".to_string()]).unwrap();
    assert!(r.is_none(), "nothing staged → no commit, no error");
    std::fs::remove_dir_all(&repo).ok();
}
