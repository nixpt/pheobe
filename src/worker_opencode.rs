//! The opencode worker adapter (PHEOBE-4): `PHEOBE_PROVIDER=opencode`.
//!
//! Surface 2 of DESIGN.md §"OpenCode integration": pheobe keeps the
//! mechanical half of the loop (intake gate, worktree, aging ladder,
//! `done_when`, allowlist, pathspec commit, report) and delegates the
//! whole turn loop to `opencode run --format json` — one prompt in (the
//! protocol envelope `agent::build_prompt` produces), one whole run back.
//!
//! argv follows mayfly's proven shape (`mayfly/src/adapters/opencode.rs`)
//! and was re-verified against the local `opencode run --help` (1.18.31):
//! `run --format json --auto [-m MODEL] [--attach URL] --dir <worktree> <prompt>`
//! — the prompt is the message positional, and the child's cwd is the
//! worktree. `--auto` mirrors mayfly: without it a `permission.asked`
//! prompt would hang a headless run; pheobe's own allowlist gate still
//! has the final word on whatever the engine touches.
//!
//! Event stream (verified from opencode source, `cli/cmd/run.ts` +
//! `session/processor.ts`): newline-delimited JSON objects of the shape
//! `{type, timestamp, sessionID, ...data}` where the mirrored events are
//! `step_start` / `text` / `step_finish` / `tool_use` / `error`, each
//! carrying the `part`. `text` parts carry assistant prose (`part.text`,
//! finalized once `part.time.end` exists); `step_finish` parts carry
//! `part.tokens {input, output, reasoning, cache.read, cache.write}` and
//! `part.cost`. Env contract: `PHEOBE_OPENCODE_BIN` (default `opencode`),
//! `PHEOBE_OPENCODE_MODEL`, `PHEOBE_OPENCODE_URL` (`--attach`), and
//! `PHEOBE_OPENCODE_TIMEOUT_SECS` (default 600) — the subprocess must not
//! hang forever even though the aging ladder wraps it too. The child
//! inherits the environment (model creds come from opencode's own auth).

use crate::worker::{Worker, WorkerCtx, WorkerOutcome};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

const DEFAULT_BIN: &str = "opencode";
const DEFAULT_TIMEOUT_SECS: u64 = 600;

pub struct OpenCodeWorker {
    bin: String,
    model: Option<String>,
    attach: Option<String>,
    timeout: Duration,
}

impl OpenCodeWorker {
    pub fn from_env() -> Result<Self> {
        let nonempty = |s: &String| !s.trim().is_empty();
        let bin = std::env::var("PHEOBE_OPENCODE_BIN")
            .ok()
            .filter(nonempty)
            .unwrap_or_else(|| DEFAULT_BIN.to_string());
        let model = std::env::var("PHEOBE_OPENCODE_MODEL").ok().filter(nonempty);
        let attach = std::env::var("PHEOBE_OPENCODE_URL").ok().filter(nonempty);
        let timeout_secs = std::env::var("PHEOBE_OPENCODE_TIMEOUT_SECS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        Ok(Self {
            bin,
            model,
            attach,
            timeout: Duration::from_secs(timeout_secs),
        })
    }
}

/// Registry hook: `("opencode", worker as WorkerFactory)` in `worker::REGISTRY`.
pub fn worker() -> Result<Arc<dyn Worker>> {
    Ok(Arc::new(OpenCodeWorker::from_env()?))
}

impl Worker for OpenCodeWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        self.run_with(prompt, worktree, &WorkerCtx::default())
    }

    /// PHEOBE-46: model (`PHEOBE_OPENCODE_MODEL` > task), ttl-bounded timeout,
    /// and pheobe's bwrap confinement in `moderate` (opencode has no sandbox of its own).
    fn run_with(&self, prompt: &str, worktree: &Path, ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        let model = crate::engine::resolve_model(self.model.clone(), ctx.model.as_deref());
        let timeout = Duration::from_secs(crate::engine::effective_timeout(
            self.timeout.as_secs(),
            ctx.ttl,
        ));
        let mut args: Vec<String> = vec![
            "run".into(),
            "--format".into(),
            "json".into(),
            "--auto".into(),
        ];
        if let Some(m) = &model {
            args.extend(["-m".into(), m.clone()]);
        }
        if let Some(u) = &self.attach {
            args.extend(["--attach".into(), u.clone()]);
        }
        args.push("--dir".into());
        args.push(worktree.display().to_string());
        // the message positional: one argument, whole envelope
        args.push(prompt.to_string());

        let mut cmd = crate::engine::command(
            "opencode",
            &self.bin,
            &args,
            worktree,
            ctx.sandbox.as_ref(),
            crate::engine::OPENCODE_HOME_RW,
        )?;
        cmd.current_dir(worktree);
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
        let mut child = match crate::worker::spawn_retry(&mut cmd) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => bail!(
                "opencode worker: binary '{}' not found on PATH — set PHEOBE_OPENCODE_BIN \
                 to the opencode binary, or install opencode",
                self.bin
            ),
            Err(e) => {
                return Err(e)
                    .with_context(|| format!("opencode worker: failed to spawn '{}'", self.bin))
            }
        };

        // drain stdout/stderr on side threads so the pipe never blocks the
        // timeout loop; each reader finishes at EOF (exit or kill).
        let out_buf = Arc::new(std::sync::Mutex::new(String::new()));
        let err_buf = Arc::new(std::sync::Mutex::new(String::new()));
        let out_reader = spawn_drain(
            child.stdout.take().expect("stdout piped"),
            Arc::clone(&out_buf),
        );
        let err_reader = spawn_drain(
            child.stderr.take().expect("stderr piped"),
            Arc::clone(&err_buf),
        );

        let started = Instant::now();
        let status = loop {
            if let Some(st) = child.try_wait()? {
                break st;
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                bail!(
                    "opencode worker: '{}' exceeded the subprocess timeout of {}s \
                     (kill applied; raise PHEOBE_OPENCODE_TIMEOUT_SECS if the task \
                     legitimately needs longer)",
                    self.bin,
                    timeout.as_secs()
                );
            }
            std::thread::sleep(Duration::from_millis(50));
        };
        let _ = out_reader.join();
        let _ = err_reader.join();
        let stdout = out_buf.lock().expect("stdout buffer").clone();
        let stderr = err_buf.lock().expect("stderr buffer").clone();

        let (final_text, tokens, usd, stream_error) = parse_stream(&stdout);
        if !status.success() {
            let mut why = format!("opencode run exited with {status}");
            if let Some(e) = stream_error {
                why.push_str(&format!("; engine reported: {e}"));
            }
            let tail: String = stderr
                .lines()
                .rev()
                .take(5)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if !tail.trim().is_empty() {
                why.push_str(&format!("; stderr: {tail}"));
            }
            bail!("{why}");
        }

        let json_tail = crate::worker::extract_json_tail(&final_text);
        Ok(WorkerOutcome {
            final_text,
            tokens,
            usd,
            json_tail,
            turns: None,
        })
    }
}

/// Move a pipe onto a thread that reads until EOF into the shared buffer.
fn spawn_drain(
    mut pipe: impl Read + Send + 'static,
    buf: Arc<std::sync::Mutex<String>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut s = String::new();
        let _ = pipe.read_to_string(&mut s);
        *buf.lock().expect("pipe buffer") = s;
    })
}

/// One pass over the `--format json` event stream: last finalized `text`
/// part wins as `final_text`; `step_finish` parts add their tokens
/// (input + output + reasoning + cache read/write) and cost.
fn parse_stream(raw: &str) -> (String, Option<u64>, Option<f64>, Option<String>) {
    let mut final_text = String::new();
    let mut tokens: u64 = 0;
    let mut saw_tokens = false;
    let mut usd = 0.0f64;
    let mut saw_usd = false;
    let mut error: Option<String> = None;

    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // non-JSON noise (banners, warnings) is ignored, not fatal
        let Ok(ev) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ev_type = ev.get("type").and_then(|t| t.as_str());
        if ev_type == Some("error") {
            let e = &ev["error"];
            let name = e.get("name").and_then(|n| n.as_str()).unwrap_or("error");
            let msg = e
                .get("data")
                .and_then(|d| d.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or(name);
            error = Some(match &error {
                Some(prev) => format!("{prev}; {msg}"),
                None => msg.to_string(),
            });
            continue;
        }
        let Some(part) = ev.get("part") else { continue };
        match part.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if part.get("time").and_then(|t| t.get("end")).is_some() {
                    if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                        final_text = text.to_string();
                    }
                }
            }
            Some("step-finish") => {
                if let Some(tk) = part.get("tokens") {
                    let sum = tk.get("input").and_then(|v| v.as_u64()).unwrap_or(0)
                        + tk.get("output").and_then(|v| v.as_u64()).unwrap_or(0)
                        + tk.get("reasoning").and_then(|v| v.as_u64()).unwrap_or(0)
                        + tk.get("cache")
                            .map(|c| {
                                c.get("read").and_then(|v| v.as_u64()).unwrap_or(0)
                                    + c.get("write").and_then(|v| v.as_u64()).unwrap_or(0)
                            })
                            .unwrap_or(0);
                    tokens += sum;
                    saw_tokens = true;
                }
                if let Some(cost) = part.get("cost").and_then(|c| c.as_f64()) {
                    usd += cost;
                    saw_usd = true;
                }
            }
            _ => {}
        }
    }
    (
        final_text,
        saw_tokens.then_some(tokens),
        saw_usd.then_some(usd),
        error,
    )
}
