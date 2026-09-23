//! The `Worker` seam (PHEOBE-9): one prompt in, a whole turn-loop out.
//!
//! Industry precedent: Vercel's `@ai-sdk/harness` + harness adapters — a
//! `HarnessAgent` connected to each coding agent. pheobe's `Worker` is the
//! same idea at the contract level: the engine (opencode, claude, cursor,
//! codex, kimi — PHEOBE-4..8) does its own internal loop and returns prose;
//! pheobe's mechanical gates (allowlist, commit, done_when) own the result.
//!
//! The adapter parses whatever engine-tail JSON it can out of the engine's
//! final output into `json_tail`; report normalization (which keys merge into
//! the handoff report and which stay mechanical) lives in `agent::run_worker`.

use anyhow::{bail, Result};
use serde_json::Value;
use std::path::Path;
use std::process::{Child, Command, Output};
use std::sync::Arc;
use std::time::Duration;

/// What a worker engine reports back from one whole run.
#[derive(Debug, Clone)]
pub struct WorkerOutcome {
    /// The engine's final prose output (its "last words").
    pub final_text: String,
    /// Total tokens the engine reported, if it reports usage.
    pub tokens: Option<u64>,
    /// Total USD cost the engine reported, if it reports cost.
    pub usd: Option<f64>,
    /// Handoff-shaped JSON the adapter extracted from the engine's tail
    /// (e.g. a final `{"ok":..,"summary":..}` block), if any.
    pub json_tail: Option<Value>,
}

/// The engine's final message may itself be the handoff contract: bare
/// JSON, a ```json fence with prose around it (claude, issue 06), or an
/// object embedded in a longer answer. Try the whole text, then the LAST
/// fence, then the widest `{...}` span. Only handoff-shaped objects count
/// (`ok` or `summary` present); an empty `blocked` is the model stating
/// "nothing blocks me" and is dropped, since agent::normalize honors presence.
pub fn extract_json_tail(final_text: &str) -> Option<Value> {
    let t = final_text.trim();
    let shaped = |v: Value| -> Option<Value> {
        if !v.is_object() || (v.get("ok").is_none() && v.get("summary").is_none()) {
            return None;
        }
        let mut v = v;
        if v.get("blocked").and_then(|b| b.as_str()) == Some("") {
            v.as_object_mut().unwrap().remove("blocked");
        }
        Some(v)
    };
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return shaped(v);
    }
    if let Some(start) = t.rfind("```json") {
        let body = &t[start + 7..];
        if let Some(end) = body.find("```") {
            if let Ok(v) = serde_json::from_str::<Value>(body[..end].trim()) {
                if let Some(v) = shaped(v) {
                    return Some(v);
                }
            }
        }
    }
    if let (Some(start), Some(end)) = (t.find('{'), t.rfind('}')) {
        if start < end {
            if let Ok(v) = serde_json::from_str::<Value>(&t[start..=end]) {
                return shaped(v);
            }
        }
    }
    None
}

/// Per-run knobs a worker MAY honour (PHEOBE-41/43). Adapters that ignore
/// them keep working unchanged: `run_with` defaults to `run`.
#[derive(Debug, Clone, Default)]
pub struct WorkerCtx {
    /// Engine model id (task `model`, or the adapter's own env override).
    pub model: Option<String>,
    /// The task's ttl: an adapter's subprocess timeout should not outlive it.
    pub ttl: Option<std::time::Duration>,
    /// The resolved sandbox tier (`PHEOBE_SANDBOX` → task → moderate).
    pub sandbox: Option<crate::sandbox::Tier>,
}

/// One prompt out, one whole run back. The engine owns its internal loop;
/// pheobe owns everything mechanical around it (aging ladder, budget, gates).
pub trait Worker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome>;

    /// `run` with per-run knobs. Default: ignore them (adapters opt in).
    fn run_with(&self, prompt: &str, worktree: &Path, _ctx: &WorkerCtx) -> Result<WorkerOutcome> {
        self.run(prompt, worktree)
    }
}

/// Registry: `PHEOBE_PROVIDER` name → worker constructor. Adapters
/// (PHEOBE-4..8) register by adding one entry here — no match-arm growth
/// anywhere else, and hosts resolve through this one place.
type WorkerFactory = fn() -> Result<Arc<dyn Worker>>;
const REGISTRY: &[(&str, WorkerFactory)] = &[
    ("opencode", crate::worker_opencode::worker as WorkerFactory), // PHEOBE-4
    ("claude", crate::worker_claude::worker as WorkerFactory),     // PHEOBE-5
    ("codex", crate::worker_codex::worker as WorkerFactory),       // PHEOBE-7
    ("cursor", crate::worker_cursor::worker as WorkerFactory),     // PHEOBE-6
    ("kimi", crate::worker_kimi::worker as WorkerFactory),         // PHEOBE-8
    ("agy", crate::worker_agy::worker as WorkerFactory),           // PHEOBE-25
    ("antigravity", crate::worker_agy::worker as WorkerFactory),   // PHEOBE-25 alias
];

/// Resolve a `PHEOBE_PROVIDER` name. `Ok(None)` = the built-in per-turn
/// `Provider` loop (openai, the default). `Ok(Some(_))` = a registered
/// worker adapter. Unknown name = a clear error naming the culprit.
pub fn worker_from_env(provider: &str) -> Result<Option<Arc<dyn Worker>>> {
    let name = provider.trim();
    match name {
        "" | "openai" => Ok(None),
        other => match REGISTRY.iter().find(|(n, _)| *n == other) {
            Some((_, make)) => Ok(Some(make()?)),
            None => {
                let known: Vec<&str> = REGISTRY.iter().map(|(n, _)| *n).collect();
                bail!(
                    "unknown PHEOBE_PROVIDER '{other}' — no worker adapter registered \
                     under that name (registered workers: [{}]; the built-in loop is 'openai')",
                    known.join(", ")
                )
            }
        },
    }
}

/// Linux/macOS `ETXTBSY` — exec of a file still open for write (issue 10).
pub(crate) fn is_etxtbsy(err: &std::io::Error) -> bool {
    err.raw_os_error() == Some(26)
}

/// `Command::spawn`, retrying a short backoff on ETXTBSY so a parallel
/// test's leftover write-fd doesn't fail the suite (issue 10).
pub(crate) fn spawn_retry(cmd: &mut Command) -> std::io::Result<Child> {
    let mut wait = Duration::from_millis(2);
    for attempt in 0..8 {
        match cmd.spawn() {
            Err(e) if is_etxtbsy(&e) && attempt + 1 < 8 => {
                std::thread::sleep(wait);
                wait = wait.saturating_mul(2);
            }
            other => return other,
        }
    }
    cmd.spawn()
}

/// `Command::output` with the same ETXTBSY backoff as [`spawn_retry`].
pub(crate) fn output_retry(cmd: &mut Command) -> std::io::Result<Output> {
    let mut wait = Duration::from_millis(2);
    for attempt in 0..8 {
        match cmd.output() {
            Err(e) if is_etxtbsy(&e) && attempt + 1 < 8 => {
                std::thread::sleep(wait);
                wait = wait.saturating_mul(2);
            }
            other => return other,
        }
    }
    cmd.output()
}

#[cfg(test)]
mod etxtbsy_tests {
    #[test]
    fn errno_26_is_etxtbsy() {
        assert!(super::is_etxtbsy(&std::io::Error::from_raw_os_error(26)));
        assert!(!super::is_etxtbsy(&std::io::Error::from_raw_os_error(2)));
    }
}
