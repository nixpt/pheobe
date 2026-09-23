//! The claude worker adapter (PHEOBE-5): `PHEOBE_PROVIDER=claude`.
//!
//! One prompt out, one whole claude run back:
//! `claude -p <prompt> --output-format json <flags>` spawned with
//! cwd = the worktree (the mayfly adapter precedent, upgraded from
//! `--output-format text` to json — the Agent SDK's official subprocess
//! escape hatch — so we get usage + a structured result, not just prose).
//!
//! Report normalization is identical to the opencode adapter: claude's
//! output is treated as prose (the `result` string of its final JSON
//! object); pheobe's mechanical gates own the contract. Engine JSON merges
//! in only through `json_tail` — if the engine's final message itself
//! parses as a JSON object (the model chose to emit the handoff contract),
//! it is handed up and `agent::run_worker` merges ONLY the
//! summary/next_steps/doubts/blocked/ok keys out of it; branch/commits/
//! tests stay mechanical.
//!
//! Env:
//! - `PHEOBE_CLAUDE_BIN` (default `claude`)
//! - `PHEOBE_CLAUDE_FLAGS` — default `--dangerously-skip-permissions`
//!   (split on whitespace). Setting it REPLACES the default entirely:
//!   e.g. `PHEOBE_CLAUDE_FLAGS=ccf-mode` appends nothing else and expects
//!   the caller's env to carry the flownet auth (same pattern as mayfly's
//!   ccf harness: the wrapper env decides the credentials).
//! - `PHEOBE_CLAUDE_MODEL` (PHEOBE-41) — appends `--model <m>` WITHOUT
//!   touching the flags above. Wins over the task's `model` field (env is
//!   the operator override, the `PHEOBE_SANDBOX` precedent).
//! - `PHEOBE_CLAUDE_TIMEOUT_SECS` (default 3600) — the adapter's own
//!   subprocess timeout: a hung claude is killed here. Since PHEOBE-43 the
//!   effective timeout is min(this, task ttl), so the child dies AT the ttl
//!   instead of the aging ladder only judging the run afterwards.
//!
//! Sandbox (PHEOBE-43) — the resolved tier applies to the whole claude run:
//! - `strict`: refused up front. The claude CLI needs the network to reach
//!   its API, and strict means `--unshare-net`; silently ignoring the tier
//!   would be worse than a clear error.
//! - `moderate` (default): `bwrap` with the network shared, `$HOME`
//!   read-only except `~/.claude` (the CLI's own state); writable: the
//!   worktree, the repo's git store (the engine commits — see agent.rs), and
//!   `$CARGO_TARGET_DIR` when set; the repo's parent read-only (sibling
//!   `../x` path-deps).
//!   Without `bwrap` it degrades to a plain subprocess with a stderr note,
//!   like the bash tool's moderate tier.
//! - `free`: plain subprocess.

use crate::sandbox::Tier;
use crate::worker::{Worker, WorkerCtx, WorkerOutcome};
use anyhow::{Context, Result};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const DEFAULT_FLAGS: &str = "--dangerously-skip-permissions";
const DEFAULT_TIMEOUT_SECS: u64 = 3600;

/// Registry constructor for `worker::REGISTRY`.
pub fn worker() -> Result<std::sync::Arc<dyn Worker>> {
    Ok(std::sync::Arc::new(ClaudeWorker))
}

/// The claude adapter. Mockable at the `Worker` seam like any other.
pub struct ClaudeWorker;

impl Worker for ClaudeWorker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome> {
        self.run_with(prompt, worktree, &WorkerCtx::default())
    }

    fn run_with(&self, prompt: &str, worktree: &Path, ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        let bin = std::env::var("PHEOBE_CLAUDE_BIN").unwrap_or_else(|_| "claude".to_string());
        let flags =
            std::env::var("PHEOBE_CLAUDE_FLAGS").unwrap_or_else(|_| DEFAULT_FLAGS.to_string());
        let env_timeout = std::env::var("PHEOBE_CLAUDE_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(DEFAULT_TIMEOUT_SECS);
        let timeout = effective_timeout(env_timeout, ctx.ttl);
        let model = resolve_model(
            std::env::var("PHEOBE_CLAUDE_MODEL").ok(),
            ctx.model.as_deref(),
        );
        let argv = claude_args(prompt, &flags, model.as_deref());

        // No tier = a direct `run()` caller outside run_worker: unsandboxed,
        // exactly as before PHEOBE-43. run_worker always resolves one.
        let mut cmd = match ctx.sandbox.clone() {
            None => plain(&bin, &argv),
            Some(Tier::Strict) => anyhow::bail!(
                "claude worker: sandbox 'strict' is not supported — the claude CLI needs the \
                 network to reach its API and strict unshares it. Use PHEOBE_SANDBOX=moderate \
                 (bwrap, network on, worktree-only writes) or free."
            ),
            Some(Tier::Free) => plain(&bin, &argv),
            Some(Tier::Moderate) => match which("bwrap") {
                Some(bwrap) => {
                    let mounts = Mounts::for_worktree(worktree);
                    let mut c = Command::new(bwrap);
                    c.args(moderate_bwrap_args(&bin, &argv, worktree, &mounts));
                    c
                }
                None => {
                    eprintln!(
                        "pheobe: claude worker: bwrap not found — moderate tier degrades to a \
                         plain subprocess"
                    );
                    plain(&bin, &argv)
                }
            },
        };
        cmd.current_dir(worktree);
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = crate::worker::spawn_retry(&mut cmd).with_context(|| {
            format!(
                "claude worker: failed to spawn '{bin}' (is the claude binary on PATH? \
                 set PHEOBE_CLAUDE_BIN to override)"
            )
        })?;

        // drain pipes on threads — a piped claude must never block on a full
        // stdout buffer while we poll
        let mut out_pipe = child
            .stdout
            .take()
            .context("claude worker: no stdout pipe")?;
        let mut err_pipe = child
            .stderr
            .take()
            .context("claude worker: no stderr pipe")?;
        let stdout_reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = out_pipe.read_to_string(&mut buf);
            buf
        });
        let stderr_reader = std::thread::spawn(move || {
            let mut buf = String::new();
            let _ = err_pipe.read_to_string(&mut buf);
            buf
        });

        let status = wait_timeout::ChildExt::wait_timeout(&mut child, Duration::from_secs(timeout))
            .with_context(|| format!("claude worker: waiting on '{bin}' failed"))?
            .with_context(|| {
                format!(
                    "claude worker: '{bin}' timed out after {timeout}s and was killed \
                     (set PHEOBE_CLAUDE_TIMEOUT_SECS to adjust; the aging ladder in \
                     run_worker judges the run separately)"
                )
            })?;

        let stdout = stdout_reader.join().unwrap_or_default();
        let stderr = stderr_reader.join().unwrap_or_default();

        if !status.success() {
            anyhow::bail!(
                "claude worker: '{bin}' exited with {status}: {}",
                stderr_tail(&stderr)
            );
        }

        let (outcome, is_error) = parse_stdout(&stdout);
        if is_error {
            anyhow::bail!(
                "claude worker: the run reported is_error: {}",
                text_tail(&outcome.final_text)
            );
        }
        Ok(outcome)
    }
}

/// `-p <prompt> --output-format json <flags…> [--model m]`. The model is
/// appended, never substituted for the flags (PHEOBE-41).
pub(crate) fn claude_args(prompt: &str, flags: &str, model: Option<&str>) -> Vec<String> {
    let mut a = vec![
        "-p".to_string(),
        prompt.to_string(),
        "--output-format".to_string(),
        "json".to_string(),
    ];
    a.extend(flags.split_whitespace().map(String::from));
    if let Some(m) = model.map(str::trim).filter(|m| !m.is_empty()) {
        a.push("--model".into());
        a.push(m.to_string());
    }
    a
}

/// Env (operator) wins over the task's `model`; blank counts as unset.
pub(crate) fn resolve_model(env: Option<String>, task: Option<&str>) -> Option<String> {
    env.filter(|m| !m.trim().is_empty())
        .or_else(|| task.map(str::to_string))
        .filter(|m| !m.trim().is_empty())
}

/// min(env timeout, ttl) in whole seconds, never below 1 (PHEOBE-43).
pub(crate) fn effective_timeout(env_secs: u64, ttl: Option<Duration>) -> u64 {
    match ttl {
        Some(t) => env_secs.min(t.as_secs().max(1)),
        None => env_secs,
    }
}

fn plain(bin: &str, argv: &[String]) -> Command {
    let mut c = Command::new(bin);
    c.args(argv);
    c
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

/// Host paths the moderate sandbox needs besides the worktree.
#[derive(Debug, Default, Clone)]
pub(crate) struct Mounts {
    pub home: Option<PathBuf>,
    /// the worktree's own git dir (`.git/worktrees/<name>`) — writable
    pub git_dir: Option<PathBuf>,
    /// the shared git common dir — writable (engine commits land here)
    pub git_common: Option<PathBuf>,
    /// the main checkout's parent, so sibling `../x` path-deps resolve — read-only
    pub repo_parent: Option<PathBuf>,
    pub cargo_target: Option<PathBuf>,
}

impl Mounts {
    fn for_worktree(wt: &Path) -> Self {
        let git = |arg: &str| {
            Command::new("git")
                .arg("-C")
                .arg(wt)
                .args(["rev-parse", "--path-format=absolute", arg])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        };
        let git_common = git("--git-common-dir");
        Mounts {
            home: std::env::var_os("HOME").map(PathBuf::from),
            git_dir: git("--git-dir"),
            repo_parent: git_common
                .as_deref()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .map(Path::to_path_buf),
            git_common,
            cargo_target: std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from),
        }
    }
}

/// bwrap argv for a moderate claude run: network shared, writes confined to
/// the worktree (+ its repo's git store, `~/.claude`, `$CARGO_TARGET_DIR`).
pub(crate) fn moderate_bwrap_args(
    bin: &str,
    argv: &[String],
    wt: &Path,
    m: &Mounts,
) -> Vec<String> {
    let mut a: Vec<String> = vec!["--unshare-pid".into(), "--die-with-parent".into()];
    let mut add = |flag: &str, p: &Path| {
        let p = p.display().to_string();
        a.extend([flag.to_string(), p.clone(), p]);
    };
    for d in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt"] {
        add("--ro-bind-try", Path::new(d)); // /etc: DNS + TLS roots for the API
    }
    // pseudo-fs + private /tmp FIRST: bwrap applies mounts in order, so a
    // later --tmpfs /tmp would shadow a worktree (or bin) that lives under /tmp
    a.extend([
        "--dev".into(),
        "/dev".into(),
        "--proc".into(),
        "/proc".into(),
    ]);
    a.extend(["--tmpfs".into(), "/tmp".into()]);
    let mut add = |flag: &str, p: &Path| {
        let p = p.display().to_string();
        a.extend([flag.to_string(), p.clone(), p]);
    };
    // an absolute PHEOBE_CLAUDE_BIN must stay reachable wherever it lives
    if let Some(dir) = Path::new(bin)
        .is_absolute()
        .then(|| Path::new(bin).parent())
        .flatten()
    {
        add("--ro-bind-try", dir);
    }
    if let Some(h) = &m.home {
        add("--ro-bind-try", h);
        add("--bind-try", &h.join(".claude"));
        add("--bind-try", &h.join(".claude.json"));
    }
    if let Some(ro) = &m.repo_parent {
        add("--ro-bind-try", ro);
    }
    // the git store is WRITABLE: the worker prompt tells the engine to commit
    // its work (agent.rs), and a commit writes objects + refs into the common
    // dir, not only the per-worktree gitdir. A read-only store made a live
    // haiku run (s463) finish the task and then report itself blocked.
    for rw in [&m.git_common, &m.git_dir, &m.cargo_target]
        .into_iter()
        .flatten()
    {
        add("--bind-try", rw);
    }
    add("--bind", wt);
    a.extend(["--chdir".into(), wt.display().to_string()]);
    a.push(bin.to_string());
    a.extend(argv.iter().cloned());
    a
}

/// Token total for the budget estimator: the sum of the usage buckets claude
/// reports on its result object (input + output + both cache classes).
fn sum_usage(usage: &Value) -> Option<u64> {
    const KEYS: [&str; 4] = [
        "input_tokens",
        "output_tokens",
        "cache_read_input_tokens",
        "cache_creation_input_tokens",
    ];
    let mut total = 0u64;
    let mut any = false;
    for key in KEYS {
        if let Some(v) = usage.get(key).and_then(|v| v.as_u64()) {
            total += v;
            any = true;
        }
    }
    any.then_some(total)
}

/// Parse claude's stdout defensively: with `--output-format json` it is a
/// single result object; a stream of objects (one per line) is tolerated;
/// an unparseable tail degrades to "the whole stdout is the engine's prose".
fn parse_stdout(raw: &str) -> (WorkerOutcome, bool) {
    let trimmed = raw.trim();
    if !trimmed.is_empty() {
        if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
            return from_result_object(&v);
        }
        let mut last: Option<Value> = None;
        let mut last_result: Option<Value> = None;
        for line in trimmed.lines() {
            let line = line.trim();
            if !line.starts_with('{') {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(line) {
                if v.get("type").and_then(|t| t.as_str()) == Some("result") {
                    last_result = Some(v.clone());
                }
                last = Some(v);
            }
        }
        if let Some(v) = last_result.or(last) {
            return from_result_object(&v);
        }
    }
    (
        WorkerOutcome {
            final_text: raw.to_string(),
            tokens: None,
            usd: None,
            json_tail: None,
        },
        false,
    )
}

/// Extract a `WorkerOutcome` from a claude result JSON object. `is_error` is
/// reported separately so the caller can surface it as a hard error.
fn from_result_object(v: &Value) -> (WorkerOutcome, bool) {
    let is_error = v.get("is_error").and_then(|b| b.as_bool()).unwrap_or(false);
    let final_text = v
        .get("result")
        .map(|r| {
            r.as_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|| r.to_string())
        })
        .unwrap_or_default();
    let tokens = v.get("usage").and_then(sum_usage);
    let usd = v.get("total_cost_usd").and_then(|c| c.as_f64());
    // the engine's final message may itself be the handoff contract in JSON —
    // if it parses as an object, hand it up as json_tail (only contract keys
    // merge; see agent::run_worker)
    let json_tail = crate::worker::extract_json_tail(&final_text);
    (
        WorkerOutcome {
            final_text,
            tokens,
            usd,
            json_tail,
        },
        is_error,
    )
}

fn stderr_tail(stderr: &str) -> String {
    let t = stderr.trim();
    if t.is_empty() {
        "(no stderr)".to_string()
    } else {
        text_tail(t)
    }
}

fn text_tail(text: &str) -> String {
    const MAX: usize = 400;
    if text.chars().count() <= MAX {
        text.to_string()
    } else {
        let cut: String = text.chars().take(MAX).collect();
        format!("{cut}…")
    }
}
