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
use std::sync::Arc;

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

/// One prompt out, one whole run back. The engine owns its internal loop;
/// pheobe owns everything mechanical around it (aging ladder, budget, gates).
pub trait Worker {
    fn run(&self, prompt: &str, worktree: &Path) -> Result<WorkerOutcome>;
}

/// Registry: `PHEOBE_PROVIDER` name → worker constructor. Adapters
/// (PHEOBE-4..8) register by adding one entry here — no match-arm growth
/// anywhere else, and hosts resolve through this one place.
type WorkerFactory = fn() -> Result<Arc<dyn Worker>>;
const REGISTRY: &[(&str, WorkerFactory)] = &[
    ("opencode", crate::worker_opencode::worker as WorkerFactory), // PHEOBE-4
    ("claude", crate::worker_claude::worker as WorkerFactory), // PHEOBE-5
    ("codex", crate::worker_codex::worker as WorkerFactory), // PHEOBE-7
    ("cursor", crate::worker_cursor::worker as WorkerFactory), // PHEOBE-6
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
