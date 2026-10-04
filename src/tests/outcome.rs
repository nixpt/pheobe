//! Worker trait + dispatcher (PHEOBE-9): the scripted worker (one prompt
//! out, one outcome back), tail normalization (prose vs JSON contract),
//! ttl hard stops around the worker call, and the provider registry.

use super::worker_task;
use crate::agent::{run_worker, LoopCfg};
use crate::worker::{worker_from_env, Worker, WorkerOutcome};
use std::path::Path;
use std::time::Duration;

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
            turns: None,
        })
    }
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
            turns: None,
        },
    };
    let out = run_worker(&w, &task, &dir, "w1", "", &[], &LoopCfg::default()).unwrap();
    assert!(out.ok);
    assert_eq!(
        out.summary.as_deref(),
        Some("wrote the parser and wired the flag")
    );
    assert!(out.next_steps.is_empty());
    assert!(out.doubts.is_empty());
    assert!(out.blocked.is_none());
    assert_eq!(out.usage.turns, 1);
    assert_eq!(out.usage.total_tokens, Some(1200));

    let calls = w.calls.lock().unwrap();
    assert_eq!(calls.len(), 1, "one prompt out, one result back");
    assert!(
        calls[0].contains("done_when"),
        "worker prompt must be the shared loop prompt"
    );
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
            turns: None,
        },
    };
    let out = run_worker(&w, &task, &dir, "w2", "", &[], &LoopCfg::default()).unwrap();
    assert!(out.ok);
    assert_eq!(
        out.summary.as_deref(),
        Some("did the thing"),
        "json summary wins over prose"
    );
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
            turns: None,
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
            turns: None,
        },
    };
    let cfg = LoopCfg {
        ttl: Some(Duration::ZERO),
        ..Default::default()
    };
    let out = run_worker(&w, &task, &dir, "ttl-pre", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("ttl_exceeded"), "got: {blocked}");
    assert!(blocked.contains("task-design failure"), "got: {blocked}");
    assert!(
        w.calls.lock().unwrap().is_empty(),
        "worker must NOT be called once expired"
    );
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
                turns: None,
            })
        }
    }
    let dir = worker_dir("ttl-post");
    let task = worker_task();
    let cfg = LoopCfg {
        ttl: Some(Duration::from_millis(20)),
        ..Default::default()
    };
    let out = run_worker(&SlowWorker, &task, &dir, "ttl-post", "", &[], &cfg).unwrap();
    assert!(!out.ok);
    let blocked = out.blocked.unwrap();
    assert!(blocked.contains("ttl_exceeded"), "got: {blocked}");
    assert_eq!(
        out.summary.as_deref(),
        Some(""),
        "result must be ignored, not reported"
    );
    std::fs::remove_dir_all(&dir).ok();
}

/// (d) the registry: 'openai' resolves to the built-in loop (None); claude
/// resolves to its adapter (PHEOBE-5); an unknown PHEOBE_PROVIDER errors
/// clearly, naming the culprit.
#[test]
fn provider_registry_resolves_or_errors_clearly() {
    assert!(
        matches!(worker_from_env("openai"), Ok(None)),
        "openai = the built-in loop"
    );
    assert!(
        matches!(worker_from_env(""), Ok(None)),
        "empty = the default loop"
    );
    assert!(
        matches!(worker_from_env("claude"), Ok(Some(_))),
        "claude = its adapter (PHEOBE-5)"
    );
    assert!(
        matches!(worker_from_env("cursor"), Ok(Some(_))),
        "cursor = its adapter (PHEOBE-6)"
    );
    assert!(
        matches!(worker_from_env("kimi"), Ok(Some(_))),
        "kimi = its adapter (PHEOBE-8)"
    );
    assert!(
        matches!(worker_from_env("agy"), Ok(Some(_))),
        "agy = its adapter (PHEOBE-25)"
    );
    assert!(
        matches!(worker_from_env("antigravity"), Ok(Some(_))),
        "antigravity = its adapter (PHEOBE-25)"
    );
    let err = match worker_from_env("definitely-not-a-provider") {
        Err(e) => e,
        Ok(_) => panic!("unknown provider must error"),
    };
    let msg = format!("{err:#}");
    assert!(
        msg.contains("unknown PHEOBE_PROVIDER 'definitely-not-a-provider'"),
        "got: {msg}"
    );
    assert!(msg.contains("no worker adapter registered"), "got: {msg}");
}
