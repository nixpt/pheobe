# PHEOBE-9 — the `Worker` trait (one prompt in, whole turn-loop out)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-9 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

PHEOBE-4/5/6/7/8 (opencode, claude, cursor, codex, kimi worker adapters)
all declare "via the `Worker` trait (from PHEOBE-4)" but no such trait
exists. The seam must land first so the quartet can be built in parallel
by separate sub-agents against one interface. Industry precedent read and
saved: Vercel's `@ai-sdk/harness` + harness adapters
(`/workspace/external/ai-sdk/harness-*.md`) — a `HarnessAgent` connected
to each coding agent. pheobe's `Worker` is the same idea at the contract
level: engine output is prose; pheobe's mechanical gates own the result.

## Success criteria

- [ ] `src/worker.rs`: `pub trait Worker { fn run(&self, prompt: &str,
      worktree: &Path) -> Result<WorkerOutcome> }` where `WorkerOutcome`
      carries `{ final_text: String, tokens: Option<u64>, usd:
      Option<f64>, json_tail: Option<Value> }`.
- [ ] `agent::run` gains a dispatcher: `PHEOBE_PROVIDER=openai` (default)
      → existing `Provider` per-turn loop; any `Worker`-backed provider →
      one prompt out, one result back, mechanical gates after. Same
      aging ladder and budget guards wrap both paths.
- [ ] The Worker path is mocked end-to-end (scripted engine producing a
      prose tail + a JSON tail variant) — report normalization: engine
      JSON (if parseable) merges into summary/next_steps/doubts only;
      branch/commits/tests stay mechanical.
- [ ] `PHEOBE_PROVIDER` names resolve via a registry so adapters
      register themselves (no match-arm growth per host).
