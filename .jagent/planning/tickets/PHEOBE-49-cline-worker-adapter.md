# PHEOBE-49 — cline worker adapter

**Filed:** 2026-09-24, foreman. Needed for the captain's multi-harness derby
(opencode, codex, cursor, cline, commandcode under one pheobe wrapper).

## Problem
cline had no pheobe worker and no adoption kit. It could only run bare through
`agent-launch --runner cline`, without pheobe's intake gate, worktree,
allowlist, `done_when` and report. A derby result would then say as much about
the missing wrapper as about the engine.

## Fix
`src/worker_cline.rs` (`PHEOBE_PROVIDER=cline`) drives cline 3.x's headless
act mode: `cline <prompt> --json --auto-approve true -c <worktree> [-t s]
[-P provider] [-m model] [--data-dir d] <flags>`. The event stream was verified
live against 3.0.62. The adapter reads the final `run_result`:
- `text` is the engine's prose, and `json_tail` is extracted from it;
- `aggregateUsage` gives tokens and `totalCost`;
- `iterations` gives turns;
- `finishReason: "error"` is a hard error that surfaces `text`, such as a
  provider quota message.

Env: `PHEOBE_CLINE_{BIN,PROVIDER,MODEL,DATA_DIR,FLAGS,TIMEOUT_SECS}`. The model
in env wins over the task's `model`. min(timeout, ttl) bounds both our wait and
cline's own `-t`. Sandbox tiers: `strict` is refused, `moderate` is bwrap with
`~/.cline` writable, `free` is a plain subprocess.

## Verified
- 8 tests in `src/tests/cline.rs`:
  - registry;
  - argv shape;
  - parser: success, error and prose fallback;
  - fake-binary runs: argv, cwd and env knobs, a quota failure surfaced
    verbatim, a crash with no result, and strict refused without spawning.
- Gates: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`, full
  `cargo test` (225 + 10), `cargo package --no-verify`.
- Live: cline's default `cline-pass` provider is out of monthly quota on this
  box (it resets in about 4 days). The adapter surfaced that exact message. A
  live end-to-end run needs another provider (`PHEOBE_CLINE_PROVIDER`) or the
  quota reset.
