# PHEOBE-52 — bro worker adapter (`PHEOBE_PROVIDER=bro`)

**Filed:** 2026-10-15, foreman (s481), at the captain's direction.
**Depends on:** bro-cli BRO-134 (`bro exec` claude-compatible result record + engine flags).
**Siblings:** mayfly MAYFLY-14 (bro harness), squadron SQ-241 (agent-launch `--engine bro`).

## Problem
bro (`nixpt/bro-cli`, a Rust agent harness on flownet/uno) can't be a pheobe engine. It got dispatched as a raw `agent-launch --runner bro` horse instead, where:
- it committed fine;
- it could never push, because its exec sandbox has no git credentials;
- git prompted for a username on the launch terminal.

pheobe's model fits bro exactly: pheobe provisions the worktree, gates on `done_when`, commits mechanically, and never pushes. bro on a local or zorro model through flownet would also be the fleet's free engine tier.

## Scope
1. `src/worker_bro.rs`, modelled on `worker_claude.rs`:
   - spawn `bro exec --cwd <worktree> [flags] <prompt>` with cwd = the worktree and stdin null;
   - drain the pipes on threads, with a timeout;
   - parse the BRO-134 record: the same `type:"result"`, `is_error`, `result`, `usage`, `total_cost_usd` keys, so `from_result_object` can be shared or copied;
   - keep `json_tail` handling identical.
2. Env:
   - `PHEOBE_BRO_BIN` (default `bro`);
   - `PHEOBE_BRO_FLAGS`: default `--sandbox process`, because pheobe's own bwrap tier is the sandbox and bro's default `cvm` has no network for builds. Setting it replaces the default;
   - `PHEOBE_BRO_MODEL`, passed as `-m`;
   - `PHEOBE_BRO_TIMEOUT_SECS` (default 3600).
3. Register `("bro", crate::worker_bro::worker)` in `worker::REGISTRY`.
4. `adopt/bro/`: a kit for bro running pheobe's loop in host mode. bro reads the protocol and ends on `pheobe verify task.json`. `pheobe adopt bro` prints it.
5. Tests: a mock `bro` script on PATH (as the claude adapter's tests do) for success, `is_error`, a non-zero exit, timeout, and a garbage stdout fallback.

## Done when
- `PHEOBE_PROVIDER=bro pheobe run task.json` on a toy task produces a committed branch and an ok report, recorded in the PR.
- The new tests pass, along with the full suite.
- README provider table and adopt list include bro.
