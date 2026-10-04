//! The claude-sdk worker (PHEOBE-45): `PHEOBE_PROVIDER=claude-sdk`.
//!
//! Same engine as the `claude` worker, driven the way the Claude Agent SDK
//! drives it: `claude --output-format stream-json --input-format stream-json
//! --permission-prompt-tool stdio --setting-sources=` with pheobe answering
//! the control protocol (`claude_proto`). What that buys over `claude -p
//! --dangerously-skip-permissions`:
//! - **per-tool policy**: every Write/Edit/Bash comes back as `can_use_tool`
//!   and is answered mechanically (worktree ∩ paths_allow, screened bash);
//!   reads are path-checked through a PreToolUse `hook_callback`;
//! - **real cost**: `usage.usd` = the result's `total_cost_usd`, `turns` =
//!   `num_turns`; `budget.max_usd` → `--max-budget-usd`;
//! - **clean TTL**: at the deadline pheobe sends `interrupt`, waits a grace
//!   period for the result, then kills;
//! - **isolation**: `--setting-sources=` so the host's ~/.claude hooks and
//!   settings don't run inside the worker (they did, without it: s463).
//!
//! The subprocess `claude` worker stays as the fallback. Env: the claude
//! worker's (`PHEOBE_CLAUDE_BIN`, `PHEOBE_CLAUDE_MODEL`,
//! `PHEOBE_CLAUDE_TIMEOUT_SECS`, `PHEOBE_SANDBOX`) plus
//! `PHEOBE_CLAUDE_SDK_ALLOW` (comma list of extra tools to allow outright) and
//! `PHEOBE_CLAUDE_SDK_GRACE_SECS` (interrupt grace, default 10).

use crate::claude_proto::{self as proto, Decision, Policy, RunResult};
use crate::sandbox::Tier;
use crate::worker::{Worker, WorkerCtx, WorkerOutcome};
use crate::worker_claude::{effective_timeout, moderate_bwrap_args, resolve_model, which, Mounts};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const DEFAULT_TIMEOUT_SECS: u64 = 3600;

pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(ClaudeSdkWorker))
}

pub struct ClaudeSdkWorker;

/// `claude` argv for the stream-json control protocol. Never
/// `--dangerously-skip-permissions`: permission is pheobe's answer.
pub(crate) fn sdk_args(model: Option<&str>, max_usd: Option<f64>) -> Vec<String> {
    let mut a: Vec<String> = [
        "--output-format",
        "stream-json",
        "--verbose",
        "--input-format",
        "stream-json",
        "--permission-prompt-tool",
        "stdio",
        "--setting-sources=",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
        a.extend(["--model".into(), m.to_string()]);
    }
    if let Some(usd) = max_usd.filter(|u| *u > 0.0) {
        a.extend(["--max-budget-usd".into(), format!("{usd}")]);
    }
    a
}

fn send(stdin: &mut ChildStdin, v: &Value) -> Result<()> {
    writeln!(stdin, "{v}").context("claude-sdk worker: writing to the CLI's stdin")?;
    stdin.flush().ok();
    Ok(())
}

fn policy_for(wt: &Path, ctx: &WorkerCtx) -> Policy {
    let worktree = wt.canonicalize().unwrap_or_else(|_| wt.to_path_buf());
    let mut read_roots = Vec::new();
    // the main checkout (parent of the git common dir) — sibling context reads
    if let Some(main) = Mounts::for_worktree(wt)
        .git_common
        .as_deref()
        .and_then(Path::parent)
    {
        read_roots.push(main.to_path_buf());
    }
    Policy {
        worktree,
        read_roots,
        paths_allow: ctx.paths_allow.clone(),
        extra_allow: std::env::var("PHEOBE_CLAUDE_SDK_ALLOW")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
    }
}

fn command(bin: &str, argv: &[String], wt: &Path, tier: Option<Tier>) -> Result<Command> {
    let plain = || {
        let mut c = Command::new(bin);
        c.args(argv);
        c
    };
    Ok(match tier {
        None | Some(Tier::Free) => plain(),
        Some(Tier::Strict) => bail!(
            "claude-sdk worker: sandbox 'strict' is not supported — the claude CLI needs the \
             network to reach its API. Use PHEOBE_SANDBOX=moderate or free."
        ),
        Some(Tier::Moderate) => match which("bwrap") {
            Some(bwrap) => {
                let mut c = Command::new(bwrap);
                c.args(moderate_bwrap_args(
                    bin,
                    argv,
                    wt,
                    &Mounts::for_worktree(wt)
                        .with_redirects("claude-sdk", crate::engine::CLAUDE_ENV_RW),
                ));
                c
            }
            None => {
                eprintln!("pheobe: claude-sdk worker: bwrap not found — moderate degrades to a plain subprocess");
                plain()
            }
        },
    })
}

/// Everything the control loop learned, before it becomes a `WorkerOutcome`.
#[derive(Debug, Default)]
pub(crate) struct Session {
    pub decisions: Vec<Decision>,
    pub result: Option<RunResult>,
    pub interrupted: bool,
    pub notes: Vec<String>,
}

/// Answer one CLI line. Returns the reply to write (if any); records into `s`.
pub(crate) fn handle_line(line: &str, policy: &Policy, s: &mut Session) -> Option<Value> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    match v.get("type").and_then(Value::as_str) {
        Some("control_request") => {
            let id = v.get("request_id").and_then(Value::as_str).unwrap_or("");
            let req = v.get("request").cloned().unwrap_or(Value::Null);
            match req.get("subtype").and_then(Value::as_str) {
                Some("can_use_tool") => {
                    let tool = req.get("tool_name").and_then(Value::as_str).unwrap_or("?");
                    let input = req.get("input").cloned().unwrap_or(Value::Null);
                    let d = policy.can_use_tool(tool, &input);
                    let reply = proto::permission_response(id, &d, &input);
                    s.decisions.push(d);
                    Some(reply)
                }
                Some("hook_callback") => {
                    let input = req.get("input").cloned().unwrap_or(Value::Null);
                    let tool = input
                        .get("tool_name")
                        .and_then(Value::as_str)
                        .unwrap_or("?");
                    let tool_input = input.get("tool_input").cloned().unwrap_or(Value::Null);
                    let d = policy.read_decision(tool, &tool_input);
                    let reply = proto::hook_response(id, &d);
                    s.decisions.push(d);
                    Some(reply)
                }
                other => Some(proto::error(
                    id,
                    &format!("pheobe claude-sdk worker does not handle control request {other:?}"),
                )),
            }
        }
        Some("control_response") => {
            let r = v.get("response").cloned().unwrap_or(Value::Null);
            if r.get("subtype").and_then(Value::as_str) == Some("error") {
                s.notes.push(format!(
                    "CLI rejected a control request: {}",
                    r.get("error").and_then(Value::as_str).unwrap_or("?")
                ));
            }
            None
        }
        Some("result") => {
            s.result = Some(proto::parse_result(&v));
            None
        }
        _ => None,
    }
}

/// Fold the session into the adapter's outcome: the engine's prose + tail,
/// with the policy's denials and any hard stop merged into the handoff keys.
pub(crate) fn outcome(s: &Session, max_usd: Option<f64>) -> WorkerOutcome {
    let r = s.result.clone().unwrap_or_default();
    let mut tail = crate::worker::extract_json_tail(&r.text).unwrap_or_else(|| json!({}));
    let mut doubts: Vec<Value> = tail
        .get("doubts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let denied: Vec<&Decision> = s.decisions.iter().filter(|d| !d.allow).collect();
    for d in &denied {
        doubts.push(json!(format!("policy denied {}: {}", d.tool, d.reason)));
    }
    if !s.decisions.is_empty() {
        doubts.push(json!(format!(
            "claude-sdk policy: {} allowed, {} denied",
            s.decisions.len() - denied.len(),
            denied.len()
        )));
    }
    doubts.extend(s.notes.iter().map(|n| json!(n)));
    let blocked = if s.result.is_none() {
        Some(if s.interrupted {
            "ttl_exceeded — interrupted at the deadline and no result arrived in the grace period"
                .to_string()
        } else {
            "the claude CLI ended without a result message".to_string()
        })
    } else if r.is_error || r.subtype.starts_with("error") {
        Some(format!("claude-sdk run ended '{}'", r.subtype))
    } else if let (Some(max), Some(usd)) = (max_usd, r.usd) {
        (usd > max).then(|| format!("budget_exceeded — ${usd:.4} spent, max ${max:.4}"))
    } else {
        None
    };
    if let Some(b) = blocked {
        tail["blocked"] = json!(b);
        tail["ok"] = json!(false);
    }
    if !doubts.is_empty() {
        tail["doubts"] = Value::Array(doubts);
    }
    let has_tail = tail.as_object().is_some_and(|o| !o.is_empty());
    WorkerOutcome {
        final_text: r.text,
        tokens: r.tokens,
        usd: r.usd,
        json_tail: has_tail.then_some(tail),
        turns: r.turns,
    }
}

fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

impl Worker for ClaudeSdkWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        self.run_with(prompt, worktree, &WorkerCtx::default())
    }

    fn run_with(&self, prompt: &str, wt: &Path, ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_CLAUDE_BIN").unwrap_or_else(|_| "claude".into());
        let model = resolve_model(
            std::env::var("PHEOBE_CLAUDE_MODEL").ok(),
            ctx.model.as_deref(),
        );
        let argv = sdk_args(model.as_deref(), ctx.max_usd);
        let env_timeout = std::env::var("PHEOBE_CLAUDE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        let timeout = Duration::from_secs(effective_timeout(env_timeout, ctx.ttl));
        let grace = Duration::from_secs(
            std::env::var("PHEOBE_CLAUDE_SDK_GRACE_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(10),
        );
        let policy = policy_for(wt, ctx);

        let mut cmd = command(&bin, &argv, wt, ctx.sandbox.clone())?;
        cmd.current_dir(wt)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = crate::worker::spawn_retry(&mut cmd).with_context(|| {
            format!(
                "claude-sdk worker: failed to spawn '{bin}' (set PHEOBE_CLAUDE_BIN to override)"
            )
        })?;
        let mut stdin = child
            .stdin
            .take()
            .context("claude-sdk worker: no stdin pipe")?;
        let stdout = child
            .stdout
            .take()
            .context("claude-sdk worker: no stdout pipe")?;
        let mut stderr = child
            .stderr
            .take()
            .context("claude-sdk worker: no stderr pipe")?;
        let (tx, rx) = mpsc::channel::<String>();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let err_reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf);
            buf
        });

        send(&mut stdin, &proto::initialize("pheobe_init"))?;
        send(&mut stdin, &proto::user_message(prompt))?;

        let mut s = Session::default();
        let mut deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left.max(Duration::from_millis(1))) {
                Ok(line) => {
                    if let Some(reply) = handle_line(&line, &policy, &mut s) {
                        // a CLI that already exited can't take the reply; the loop ends on EOF
                        let _ = send(&mut stdin, &reply);
                    }
                    if s.result.is_some() {
                        break;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if s.interrupted {
                        break; // grace spent: kill below
                    }
                    s.interrupted = true;
                    let _ = send(&mut stdin, &proto::interrupt("pheobe_interrupt"));
                    deadline = Instant::now() + grace;
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }
        drop(stdin); // end of input: the CLI exits after its result
        match wait_timeout::ChildExt::wait_timeout(&mut child, Duration::from_secs(10)) {
            Ok(Some(_)) => {}
            _ => kill(&mut child),
        }
        let stderr = err_reader.join().unwrap_or_default();
        if s.result.is_none() && !s.interrupted && s.decisions.is_empty() {
            let tail: String = stderr
                .trim()
                .chars()
                .rev()
                .take(400)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            bail!(
                "claude-sdk worker: '{bin}' produced no result: {}",
                if tail.is_empty() {
                    "(no stderr)".into()
                } else {
                    tail
                }
            );
        }
        Ok(outcome(&s, ctx.max_usd))
    }
}
