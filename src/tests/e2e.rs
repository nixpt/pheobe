use std::path::Path;

use super::{claude_env_lock, del_env, run_git, set_env};
use crate::task::Task;

// ── run_task end-to-end: the WHOLE pipeline over a real HTTP endpoint ───────
//
// The loop tests drive agent::run with a scripted in-process provider; this one
// drives run::run_task — the exact surface the CLI and `pheobe acp` share —
// against a one-shot OpenAI-shaped endpoint on an ephemeral port (the
// PHEOBE-17 smoke's mock, in-process and free). Real reqwest POSTs, real
// worktree provision, real commit gate, real done_when, real report.

/// A one-shot HTTP server: serves the canned chat/completions bodies in
/// order, then stops accepting. Raw std TcpListener + thread — no test-only
/// dependency surface.
struct MockOpenAi {
    base_url: String,
}

impl MockOpenAi {
    fn new(bodies: Vec<String>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            use std::io::{Read, Write};
            let mut served = 0;
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { break };
                // read headers + body (may span several recv()s — the
                // system prompt this endpoint receives is tens of KB)
                let mut head = Vec::new();
                loop {
                    let mut buf = [0u8; 8192];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    head.extend_from_slice(&buf[..n]);
                    if let Some(pos) = find_head_end(&head) {
                        let head_str = String::from_utf8_lossy(&head[..pos]).to_string();
                        let cl = head_str
                            .lines()
                            .find_map(|l| {
                                let (k, v) = l.split_once(':')?;
                                k.trim()
                                    .eq_ignore_ascii_case("content-length")
                                    .then(|| v.trim().parse::<usize>().ok())?
                            })
                            .unwrap_or(0);
                        if head.len() >= pos + 4 + cl {
                            break;
                        }
                    }
                }
                let body = bodies
                    .get(served)
                    .cloned()
                    .or_else(|| bodies.last().cloned())
                    .unwrap_or_default();
                served += 1;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
                if served >= bodies.len() {
                    break;
                }
            }
        });
        let _ = handle; // dies on its own after the last body
        MockOpenAi {
            base_url: format!("http://{addr}/v1"),
        }
    }
}

/// Offset of the blank line ending an HTTP request head, if fully received.
fn find_head_end(raw: &[u8]) -> Option<usize> {
    raw.windows(4).position(|w| w == b"\r\n\r\n")
}

/// One chat/completions body: an assistant message with a single tool call
/// (`name` = the tool, `args` = the JSON-string arguments).
fn tool_call_body(id: &str, name: &str, args: &str) -> String {
    serde_json::json!({
        "choices": [{
            "message": {
                "role": "assistant",
                "content": null,
                "tool_calls": [{
                    "id": id, "type": "function",
                    "function": {"name": name, "arguments": args}
                }]
            }
        }],
        "usage": {"total_tokens": 11}
    })
    .to_string()
}

#[test]
fn run_task_end_to_end_over_a_mock_endpoint() {
    let _e = crate::memory::tests::env_lock().lock().unwrap();
    let _g = claude_env_lock();

    // scratch source repo: a seed file + a committed knowledge entry (drives
    // the repo-local knowledge root through load_all/scan/parse_entry too)
    let root = std::env::temp_dir().join(format!("pheobe-run-task-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q"]);
    run_git(&repo, &["config", "user.email", "t@t"]);
    run_git(&repo, &["config", "user.name", "t"]);
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/lib.rs"), "pub fn seed() {}\n").unwrap();
    let kn = repo.join(".pheobe").join("knowledge");
    std::fs::create_dir_all(&kn).unwrap();
    std::fs::write(
        kn.join("entry.md"),
        "---\nslug: entry\nname: Test Entry\nkind: external\nversion: \"1\"\nlast_verified: 2026-09-16\n---\nbody\n",
    )
    .unwrap();
    run_git(&repo, &["add", "-A"]);
    run_git(&repo, &["commit", "-m", "root", "-q"]);

    // deterministic env: empty global knowledge drive, built-in openai loop
    let knowledge_dir = root.join("global-knowledge");
    std::fs::create_dir_all(&knowledge_dir).unwrap();
    set_env("PHEOBE_KNOWLEDGE_DIR", &knowledge_dir.display().to_string());
    del_env("PHEOBE_PROVIDER");
    del_env("PHEOBE_API_KEY");
    del_env("PHEOBE_SANDBOX");

    // the mock script walks the WHOLE barn: bash, read, edit, write, handoff
    let mock = MockOpenAi::new(vec![
        tool_call_body("call_b", "bash", r#"{"command":"echo e2e-bash-ran"}"#),
        tool_call_body("call_r", "read", r#"{"path":"src/lib.rs"}"#),
        tool_call_body(
            "call_e",
            "edit",
            r#"{"path":"src/lib.rs","old_string":"seed","new_string":"sprout"}"#,
        ),
        tool_call_body(
            "call_w",
            "write",
            r#"{"path":"src/x.rs","content":"pub fn x() -> u8 { 1 }\n"}"#,
        ),
        tool_call_body(
            "call_h",
            "handoff",
            r#"{"ok":true,"summary":"added src/x.rs","next_steps":["review the file"],"doubts":[]}"#,
        ),
    ]);
    set_env("PHEOBE_BASE_URL", &mock.base_url);
    set_env("PHEOBE_MODEL", "mock-model");

    let task: Task = serde_json::from_str(&format!(
        r#"{{"task":"Add src/x.rs through the run_task end-to-end test","done_when":{{"type":"command","run":"test -f src/x.rs"}},"paths_allow":["src/"],"repo":"{}","worktree":true}}"#,
        repo.display()
    ))
    .unwrap();

    let progress: std::sync::Mutex<Vec<String>> = Default::default();
    let rep = crate::run::run_task(&task, None, &|s| {
        progress.lock().unwrap().push(s.to_string())
    })
    .unwrap();

    // the report contract — the shape every adopter consumes
    assert!(rep.ok, "report should be ok, got: {rep:#?}");
    assert_eq!(rep.summary.as_deref(), Some("added src/x.rs"));
    assert_eq!(rep.next_steps, vec!["review the file".to_string()]);
    assert!(rep.blocked.is_none());
    assert_eq!(rep.usage.as_ref().unwrap().turns, 5);
    let wt = rep.worktree.clone().expect("worktree in report");
    let branch = rep.branch.clone().expect("branch in report");
    assert!(branch.starts_with("pheobe/"), "branch {branch}");
    assert!(Path::new(&wt).is_dir(), "worktree {wt} should exist");
    // (the write tool rustfmts — the content lands formatted)
    assert_eq!(
        std::fs::read_to_string(Path::new(&wt).join("src/x.rs"))
            .unwrap()
            .trim(),
        "pub fn x() -> u8 {\n    1\n}"
    );
    // the edit tool's change landed too, and rode the same commit
    assert_eq!(
        std::fs::read_to_string(Path::new(&wt).join("src/lib.rs"))
            .unwrap()
            .trim(),
        "pub fn sprout() {}"
    );
    assert_eq!(rep.commits.len(), 1, "exactly the one commit");
    let evidence = rep.tests.as_ref().expect("tests evidence");
    assert!(evidence.passed);
    assert_eq!(evidence.ran, "test -f src/x.rs");

    // the stage transition streamed to the progress callback (the ACP
    // notification path and the CLI share this)
    assert!(
        progress
            .lock()
            .unwrap()
            .iter()
            .any(|p| p.contains("🍳 worktree:")),
        "worktree stage should stream, got: {:?}",
        progress.lock().unwrap()
    );

    // provenance trailer on the commit pheobe made
    let msg = run_git(Path::new(&wt), &["log", "-1", "--format=%B"]);
    assert!(msg.contains("Pheobe-Task:"), "trailer missing: {msg}");

    // cleanup: the run's branch + worktree off the scratch repo, then dirs
    let _ = run_git(&repo, &["worktree", "remove", "--force", &wt]);
    let _ = run_git(&repo, &["branch", "-D", &branch]);
    del_env("PHEOBE_BASE_URL");
    del_env("PHEOBE_MODEL");
    del_env("PHEOBE_KNOWLEDGE_DIR");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn failure_report_shape_and_json_roundtrip() {
    let rep = crate::report::HandoffReport::failure("some task", "blocked: no sandbox");
    assert!(!rep.ok);
    assert_eq!(rep.task, "some task");
    assert_eq!(rep.blocked.as_deref(), Some("blocked: no sandbox"));
    assert!(rep.branch.is_none() && rep.worktree.is_none());
    assert!(rep.commits.is_empty() && rep.tests.is_none());
    // the JSON contract roundtrips — adopters parse this shape
    let json = serde_json::to_string(&rep).unwrap();
    assert!(json.contains("\"blocked\""));
    let back: crate::report::HandoffReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back.blocked, rep.blocked);
    assert_eq!(back.ok, rep.ok);
}
