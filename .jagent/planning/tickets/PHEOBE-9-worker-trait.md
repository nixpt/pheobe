# PHEOBE-9 — the `Worker` trait (one prompt in, whole turn-loop out)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-9 |
| **Priority** | P1 |
| **Status** | Done |
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

- [x] `src/worker.rs`: `pub trait Worker { fn run(&self, prompt: &str,
      worktree: &Path) -> Result<WorkerOutcome> }` where `WorkerOutcome`
      carries `{ final_text: String, tokens: Option<u64>, usd:
      Option<f64>, json_tail: Option<Value> }`.
- [x] `agent::run` gains a dispatcher: `PHEOBE_PROVIDER=openai` (default)
      → existing `Provider` per-turn loop; any `Worker`-backed provider →
      one prompt out, one result back, mechanical gates after. Same
      aging ladder and budget guards wrap both paths.
- [x] The Worker path is mocked end-to-end (scripted engine producing a
      prose tail + a JSON tail variant) — report normalization: engine
      JSON (if parseable) merges into summary/next_steps/doubts only;
      branch/commits/tests stay mechanical.
- [x] `PHEOBE_PROVIDER` names resolve via a registry so adapters
      register themselves (no match-arm growth per host).

## Resolution

Landed on `agent/nixp/PHEOBE-9-worker-trait` (2026-09-16).

- `src/worker.rs`: `Worker` trait + `WorkerOutcome` as specced. Registry =
  `worker_from_env(name) -> Result<Option<Arc<dyn Worker>>>`: `Ok(None)` =
  the built-in per-turn `Provider` loop (`openai`/empty, the default),
  `Ok(Some(_))` = a registered worker adapter, unknown name = clear error.
  Adapters register by adding one entry to the `REGISTRY` table — no
  match-arm growth elsewhere. (Return type is `Result` rather than the
  specced `Option` because "unknown PHEOBE_PROVIDER = clear error" needs
  the Err arm; `Ok(None)` cleanly means "not a worker, use the loop".)
- `src/agent.rs`: prompt building extracted verbatim into
  `pub fn build_prompt(...)`; both paths share it. New `pub fn
  run_worker(...)`: deadline check before AND after the single worker call
  (ttl=0s → blocked `ttl_exceeded — task-design failure`, worker never
  invoked; engine overrunning its ttl → result ignored), budget guard
  before (zero budget blocks) and after (engine-reported usd, else token
  estimate). No per-turn loop, no tools in the worker path.
- `src/main.rs` `cmd_run`: dispatch by `PHEOBE_PROVIDER` — `Ok(None)` →
  the openai per-turn loop exactly as before; `Ok(Some(w))` →
  `run_worker`; unknown → registry error. The openai path's behavior is
  unchanged (same prompt, same loop, same gates).
- Report normalization (`normalize_worker_outcome`): json_tail merges ONLY
  summary/next_steps/doubts (plus honoring an explicit `ok:false`/
  `blocked`, which are part of the handoff contract the prompt asks for);
  branch/commits/tests keys in the tail are ignored — they stay mechanical,
  filled in cmd_run. Prose-only tail → summary = final_text.
- Tests: 5 new model-free tests (scripted `ScriptedWorker` + a `SlowWorker`)
  — prose tail, JSON-tail merge-only, ttl pre-call hard stop (worker not
  called), ttl mid-call overrun (result ignored), registry resolution.
  21/21 green (16 pre-existing + 5).
