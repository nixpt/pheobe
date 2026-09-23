# PHEOBE-46 — engine parity: every worker honours model/ttl/sandbox; truthful usage; error-after-commit

**Filed:** s463 (2026-09-23), foreman — Track 1 of the multi-runner dispatch (foreman FMN-5 routine tier;
agent-launch `--runner pheobe --engine X` is squadron SQ-222). Stacked on PHEOBE-45.

## Problem (s463 dispatch scout, verified in source)
- Only the claude adapter honoured `WorkerCtx`; opencode/cursor/codex/kimi/agy ignored the task `model`,
  `ttl` and `sandbox`. codex had **no timeout**; opencode and kimi had **no sandbox**.
- `report.usage.usd` was hard-coded `None` for every worker (parsed costs dropped); `turns` always 1.
- A worker error after the engine had already committed surfaced as exit 2 "never ran".
- The engine was env-only (`PHEOBE_PROVIDER`), never per task.
- kimi handed its raw last stream object up as `json_tail` (a non-handoff object could set ok/summary).

## Done
- `src/engine.rs` (new, shared): model precedence, ttl-capped timeout, one sandbox builder (pheobe bwrap
  with per-engine writable state dirs; strict refused for network engines), `wait_or_kill`.
- Every adapter implements `run_with`: model flag (verified against the installed CLIs: opencode `-m`,
  cursor/agy `--model`, codex/kimi `-m`), `PHEOBE_<ENGINE>_MODEL` > task; timeout = min(env, ttl);
  opencode/kimi/agy-moderate get pheobe bwrap; cursor/codex/agy-strict keep their native sandbox but take
  the tier from the task (not only the env).
- **Found and fixed:** every `wait_timeout` adapter (claude, cursor, kimi, agy) errored "timed out … and was
  killed" WITHOUT killing — the engine kept running (and spending, possibly committing). `wait_or_kill`
  kills + reaps. PHEOBE-43's TTL test only asserted the error, not the kill.
- Report truth: `usage.usd` from the engine; real turns; engine error after commits → ok:false with commits,
  exit 1; before any commit → exit 2 as before.
- Task `provider` (validated; `PHEOBE_PROVIDER` > task > openai).
- kimi `json_tail` via `extract_json_tail` (handoff-shaped text only).
- Pre-existing flake fixed: kimi tests raced on the process-global `PHEOBE_KIMI_BIN`/`_FLAGS` (up to 4 failed
  when run as a group) — per-test lock.
- Contract kept: the unknown-tier error still names `PHEOBE_SANDBOX`.

## Verified
208 lib + 10 cli tests (was 193 + 10), 3 consecutive full runs; clippy -D warnings, fmt --check,
`cargo package --no-verify` clean. Live, full pipeline: `pheobe run` (built binary), task `provider:"claude-sdk"`,
`model:"haiku"`, moderate sandbox → exit 0, ok, commit 3d13ad3, usage {turns 13, usd 0.0874}.
