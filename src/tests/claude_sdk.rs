//! PHEOBE-45: the claude-sdk worker — the stream-json control protocol.
//! Policy unit tests, a conformance replay of a real (scrubbed) CLI
//! transcript, and an end-to-end run against a fake CLI that speaks the
//! protocol and records every reply pheobe writes back.

use super::{claude_env_lock, del_env, set_env};
use crate::claude_proto::{self as proto, Policy};
use crate::worker::{Worker, WorkerCtx};
use crate::worker_claude_sdk::{handle_line, outcome, sdk_args, ClaudeSdkWorker, Session};
use serde_json::{json, Value};
use std::path::PathBuf;

fn policy(wt: &str, allow: &[&str]) -> Policy {
    Policy {
        worktree: PathBuf::from(wt),
        read_roots: vec![PathBuf::from("/repo/main")],
        paths_allow: allow.iter().map(|s| s.to_string()).collect(),
        extra_allow: vec![],
    }
}

#[test]
fn sdk_args_never_skip_permissions_and_isolate_settings() {
    let a = sdk_args(Some("haiku"), Some(0.1));
    assert!(!a.iter().any(|x| x.contains("dangerously")));
    for want in [
        "stream-json",
        "--permission-prompt-tool",
        "stdio",
        "--setting-sources=",
        "haiku",
        "--max-budget-usd",
        "0.1",
    ] {
        assert!(a.iter().any(|x| x == want), "missing {want}: {a:?}");
    }
    assert!(!sdk_args(None, None)
        .iter()
        .any(|x| x == "--model" || x == "--max-budget-usd"));
}

#[test]
fn writes_confined_to_worktree_and_paths_allow() {
    let p = policy("/wt", &["src/"]);
    assert!(
        p.can_use_tool("Write", &json!({"file_path": "src/lib.rs"}))
            .allow
    );
    assert!(
        p.can_use_tool("Edit", &json!({"file_path": "/wt/src/a.rs"}))
            .allow
    );
    let out = p.can_use_tool("Write", &json!({"file_path": "README.md"}));
    assert!(!out.allow && out.reason.contains("paths_allow"), "{out:?}");
    assert!(
        !p.can_use_tool("Write", &json!({"file_path": "../escape.txt"}))
            .allow
    );
    assert!(
        !p.can_use_tool("Write", &json!({"file_path": "src/../../x"}))
            .allow
    );
    assert!(
        !p.can_use_tool("NotebookEdit", &json!({})).allow,
        "no target path"
    );
    // empty paths_allow = the whole worktree
    assert!(
        policy("/wt", &[])
            .can_use_tool("Write", &json!({"file_path": "README.md"}))
            .allow
    );
}

#[test]
fn bash_is_screened() {
    let p = policy("/wt", &[]);
    for bad in [
        "git push origin main",
        "gh pr merge 3",
        "sudo rm x",
        "git push --force",
        "rm -rf /",
        "rm -r ../sibling",
        "rm -rf ~/x",
        "rm -r .",
    ] {
        assert!(
            !p.can_use_tool("Bash", &json!({"command": bad})).allow,
            "should deny: {bad}"
        );
    }
    for ok in [
        "cargo test",
        "rm -r target/tmp",
        "git commit -m x",
        "ls -la",
    ] {
        assert!(
            p.can_use_tool("Bash", &json!({"command": ok})).allow,
            "should allow: {ok}"
        );
    }
}

/// s463 live run: a denied Write came back as `cp hello.txt copy.txt` through
/// Bash — shell writes are held to the same scope.
#[test]
fn bash_writes_are_held_to_paths_allow() {
    let p = policy("/wt", &["src/"]);
    for bad in [
        "cp hello.txt copy.txt",
        "cat hello.txt > copy.txt",
        "echo x >>notes.md",
        "cat a | tee out.txt",
        "touch README.md",
        "mkdir -p ../elsewhere",
        "mv src/a.rs b.rs",
        "cargo build && cp target/x /usr/local/bin/x",
    ] {
        let d = p.can_use_tool("Bash", &json!({"command": bad}));
        assert!(!d.allow, "should deny: {bad} ({d:?})");
    }
    for ok in [
        "cp a.txt src/a.txt",
        "cargo test > /dev/null",
        "echo hi > src/log.txt",
        "cat hello.txt",
        "grep -n x src/a.rs 2>&1",
    ] {
        let d = p.can_use_tool("Bash", &json!({"command": ok}));
        assert!(d.allow, "should allow: {ok} ({d:?})");
    }
    assert_eq!(
        proto::bash_write_targets("cp -r a b && echo x > c"),
        vec!["b", "c"]
    );
}

#[test]
fn other_tools_need_the_allowlist() {
    let p = policy("/wt", &[]);
    assert!(p.can_use_tool("TodoWrite", &json!({})).allow);
    assert!(!p.can_use_tool("WebFetch", &json!({"url": "x"})).allow);
    assert!(!p.can_use_tool("Task", &json!({})).allow);
    let mut q = p.clone();
    q.extra_allow = vec!["WebFetch".into()];
    assert!(q.can_use_tool("WebFetch", &json!({"url": "x"})).allow);
}

#[test]
fn reads_stay_in_the_worktree_or_repo() {
    let p = policy("/wt", &[]);
    assert!(
        p.read_decision("Read", &json!({"file_path": "src/x.rs"}))
            .allow
    );
    assert!(
        p.read_decision("Read", &json!({"file_path": "/repo/main/Cargo.toml"}))
            .allow
    );
    assert!(
        p.read_decision("Grep", &json!({"pattern": "x"})).allow,
        "no path = cwd"
    );
    assert!(
        !p.read_decision("Read", &json!({"file_path": "/etc/shadow"}))
            .allow
    );
    assert!(
        !p.read_decision("Glob", &json!({"path": "/home/someone"}))
            .allow
    );
}

#[test]
fn reply_shapes_match_the_sdk() {
    let d = proto::Decision {
        tool: "Write".into(),
        allow: false,
        reason: "r".into(),
    };
    let r = proto::permission_response("id1", &d, &json!({}));
    assert_eq!(r["type"], "control_response");
    assert_eq!(r["response"]["request_id"], "id1");
    assert_eq!(r["response"]["response"]["behavior"], "deny");
    let a = proto::Decision {
        allow: true,
        ..d.clone()
    };
    let r = proto::permission_response("id2", &a, &json!({"file_path": "f"}));
    assert_eq!(r["response"]["response"]["updatedInput"]["file_path"], "f");
    let h = proto::hook_response("h1", &d);
    assert_eq!(
        h["response"]["response"]["hookSpecificOutput"]["permissionDecision"],
        "deny"
    );
    assert_eq!(
        proto::hook_response("h2", &a)["response"]["response"],
        json!({})
    );
    let init = proto::initialize("i");
    assert_eq!(
        init["request"]["hooks"]["PreToolUse"][0]["hookCallbackIds"][0],
        proto::READ_HOOK_ID
    );
}

/// Conformance: a real claude CLI transcript (s463, haiku, scrubbed) replays
/// through handle_line — every control_request gets a well-formed reply and
/// the result parses with a real cost. Re-run when the claude CLI moves.
#[test]
fn real_transcript_fixture_replays() {
    let fixture = include_str!("../../tests/fixtures/claude-stream-json.jsonl");
    let p = policy("/wt", &["src/"]);
    let mut s = Session::default();
    let mut replies = vec![];
    for line in fixture.lines() {
        if let Some(r) = handle_line(line, &p, &mut s) {
            replies.push(r);
        }
    }
    assert!(!replies.is_empty(), "the fixture carries control requests");
    for r in &replies {
        assert_eq!(r["type"], "control_response");
        assert_eq!(r["response"]["subtype"], "success");
    }
    // the model's Write of copy.txt is outside paths_allow → denied
    let write = s
        .decisions
        .iter()
        .find(|d| d.tool == "Write")
        .expect("a Write request");
    assert!(!write.allow);
    let res = s.result.clone().expect("result parsed");
    assert!(!res.is_error && res.subtype == "success");
    assert!(res.usd.unwrap() > 0.0 && res.turns.unwrap() >= 1);
    let o = outcome(&s, Some(0.10));
    assert_eq!(o.usd, res.usd);
    assert_eq!(o.turns, res.turns);
    let doubts = o.json_tail.unwrap()["doubts"].to_string();
    assert!(doubts.contains("policy denied Write"), "{doubts}");
}

#[test]
fn outcome_blocks_on_error_budget_and_missing_result() {
    let mut s = Session {
        result: Some(proto::RunResult {
            subtype: "error_max_turns".into(),
            ..Default::default()
        }),
        ..Default::default()
    };
    assert!(outcome(&s, None).json_tail.unwrap()["blocked"]
        .as_str()
        .unwrap()
        .contains("error_max_turns"));
    s.result = Some(proto::RunResult {
        subtype: "success".into(),
        usd: Some(0.5),
        ..Default::default()
    });
    assert!(outcome(&s, Some(0.1)).json_tail.unwrap()["blocked"]
        .as_str()
        .unwrap()
        .contains("budget_exceeded"));
    assert!(
        outcome(&s, Some(1.0)).json_tail.is_none(),
        "clean run: nothing to add"
    );
    let s = Session {
        interrupted: true,
        ..Default::default()
    };
    assert!(outcome(&s, None).json_tail.unwrap()["blocked"]
        .as_str()
        .unwrap()
        .contains("ttl_exceeded"));
}

/// A fake claude CLI that speaks the protocol: acks initialize, asks about a
/// Write outside paths_allow, a Write inside, a git push, and (as the read
/// hook) a Read outside the repo — writing each of pheobe's replies to a file
/// — then emits a result with a cost.
const FAKE_CLI: &str = r#"read -r init; printf '%s\n' "$init" >> "$PHEOBE_FAKE_REPLIES"
printf '%s\n' '{"type":"control_response","response":{"subtype":"success","request_id":"pheobe_init"}}'
read -r user; printf '%s\n' "$user" >> "$PHEOBE_FAKE_REPLIES"
printf '%s\n' '{"type":"system","subtype":"init","model":"fake"}'
printf '%s\n' '{"type":"control_request","request_id":"c1","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"README.md","content":"x"}}}'
read -r r; printf '%s\n' "$r" >> "$PHEOBE_FAKE_REPLIES"
printf '%s\n' '{"type":"control_request","request_id":"c2","request":{"subtype":"can_use_tool","tool_name":"Write","input":{"file_path":"src/lib.rs","content":"x"}}}'
read -r r; printf '%s\n' "$r" >> "$PHEOBE_FAKE_REPLIES"
printf '%s\n' '{"type":"control_request","request_id":"c3","request":{"subtype":"can_use_tool","tool_name":"Bash","input":{"command":"git push origin main"}}}'
read -r r; printf '%s\n' "$r" >> "$PHEOBE_FAKE_REPLIES"
printf '%s\n' '{"type":"control_request","request_id":"h1","request":{"subtype":"hook_callback","callback_id":"pheobe_read_guard","input":{"tool_name":"Read","tool_input":{"file_path":"/etc/shadow"}}}}'
read -r r; printf '%s\n' "$r" >> "$PHEOBE_FAKE_REPLIES"
printf '%s\n' '{"type":"result","subtype":"success","is_error":false,"num_turns":5,"result":"done","total_cost_usd":0.0123,"usage":{"input_tokens":100,"output_tokens":20}}'
"#;

#[test]
fn end_to_end_against_a_protocol_speaking_fake_cli() {
    let _g = claude_env_lock();
    let dir = std::env::temp_dir().join(format!("pheobe-claude-sdk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("wt/src")).unwrap();
    let script = super::write_shim(&dir, "fake-claude-sdk", FAKE_CLI);
    let replies = dir.join("replies.jsonl");
    set_env("PHEOBE_CLAUDE_BIN", &script.display().to_string());
    set_env("PHEOBE_FAKE_REPLIES", &replies.display().to_string());
    let ctx = WorkerCtx {
        paths_allow: vec!["src/".into()],
        max_usd: Some(0.5),
        ..Default::default()
    };
    let out = ClaudeSdkWorker.run_with("do it", &dir.join("wt"), &ctx);
    del_env("PHEOBE_CLAUDE_BIN");
    del_env("PHEOBE_FAKE_REPLIES");
    let o = out.expect("worker ran");
    assert_eq!(o.usd, Some(0.0123));
    assert_eq!(o.turns, Some(5));
    assert_eq!(o.tokens, Some(120));

    let sent: Vec<Value> = std::fs::read_to_string(&replies)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(sent[0]["request"]["subtype"], "initialize");
    assert_eq!(sent[1]["type"], "user");
    let behavior = |i: usize| {
        sent[i]["response"]["response"]["behavior"]
            .as_str()
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(behavior(2), "deny", "Write outside paths_allow");
    assert_eq!(behavior(3), "allow", "Write inside paths_allow");
    assert_eq!(behavior(4), "deny", "git push");
    assert_eq!(
        sent[5]["response"]["response"]["hookSpecificOutput"]["permissionDecision"], "deny",
        "Read outside the repo"
    );
    let doubts = o.json_tail.unwrap()["doubts"].to_string();
    assert!(doubts.contains("1 allowed, 3 denied"), "{doubts}");
}

/// Live, against the real claude CLI (costs a few cents): ignored unless
/// PHEOBE_LIVE_CLAUDE=1. A Write outside paths_allow must not land on disk,
/// and the cost must come back.
#[test]
#[ignore]
fn live_claude_sdk_denies_out_of_scope_write() {
    if std::env::var("PHEOBE_LIVE_CLAUDE").as_deref() != Ok("1") {
        return;
    }
    let dir = std::env::temp_dir().join(format!("pheobe-live-sdk-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("hello.txt"), "the answer is 42\n").unwrap();
    let ctx = WorkerCtx {
        model: Some("haiku".into()),
        paths_allow: vec!["src/".into()],
        max_usd: Some(0.10),
        ..Default::default()
    };
    let o = ClaudeSdkWorker
        .run_with("Read hello.txt, then write its contents to copy.txt in this directory. Reply in one line.", &dir, &ctx)
        .expect("live run");
    eprintln!(
        "live: usd={:?} turns={:?} tail={:?}",
        o.usd, o.turns, o.json_tail
    );
    assert!(
        !dir.join("copy.txt").exists(),
        "out-of-scope write landed on disk"
    );
    assert!(o.usd.is_some_and(|u| u > 0.0));
}

/// PHEOBE-47 live (gated): claude-sdk under the MODERATE sandbox with CLAUDE_CONFIG_DIR
/// redirected outside ~/.claude — the exact s463 failure ("Not logged in"). Set
/// PHEOBE_LIVE_CLAUDE=1 and PHEOBE_LIVE_CLAUDE_CONFIG_DIR=<a logged-in config dir>.
#[test]
fn live_claude_sdk_moderate_with_redirected_config_dir() {
    if std::env::var("PHEOBE_LIVE_CLAUDE").as_deref() != Ok("1") {
        return;
    }
    let Ok(cfg) = std::env::var("PHEOBE_LIVE_CLAUDE_CONFIG_DIR") else {
        eprintln!("skip: set PHEOBE_LIVE_CLAUDE_CONFIG_DIR");
        return;
    };
    std::env::set_var("CLAUDE_CONFIG_DIR", &cfg);
    let dir = std::env::temp_dir().join(format!("pheobe-live-47-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let ctx = WorkerCtx {
        model: Some("haiku".into()),
        sandbox: Some(crate::sandbox::Tier::Moderate),
        paths_allow: vec!["hi.txt".into()],
        max_usd: Some(0.10),
        ..Default::default()
    };
    let o = ClaudeSdkWorker
        .run_with(
            "Create a file hi.txt in this directory containing the word hi. Reply in one line.",
            &dir,
            &ctx,
        )
        .expect("live run");
    std::env::remove_var("CLAUDE_CONFIG_DIR");
    eprintln!(
        "live47: usd={:?} turns={:?} tail={:?}",
        o.usd, o.turns, o.json_tail
    );
    assert!(
        !format!("{:?}", o.json_tail).contains("Not logged in"),
        "still not logged in"
    );
    assert!(dir.join("hi.txt").exists(), "engine did not write hi.txt");
    assert!(o.usd.is_some_and(|u| u > 0.0));
    std::fs::remove_dir_all(&dir).ok();
}
