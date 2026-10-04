# PHEOBE-45: `claude-sdk` worker: the Agent SDK's control protocol, spoken from Rust

**Filed:** s463 (2026-09-23), foreman, captain-approved ("option A"). Stacked on PHEOBE-41..44 (PR #7).

## Why
The Claude Agent SDK is Python/TypeScript only, and pheobe is a single Rust binary. The SDK itself is a
wrapper: it runs `claude --output-format stream-json --input-format stream-json --verbose` and exchanges
a JSON control protocol over stdio (claude-agent-sdk-python `_internal/query.py`). Speaking that protocol
directly gives pheobe the SDK's control with no new runtime: per-tool permission answers instead of
`--dangerously-skip-permissions`, real cost, and a clean interrupt.

## Done
- `src/claude_proto.rs`: wire shapes + `Policy` (pure, unit-tested); `bash_write_targets`.
- `src/worker_claude_sdk.rs`: `PHEOBE_PROVIDER=claude-sdk`. Spawn (moderate bwrap per PHEOBE-43; strict
  refused), initialize with a PreToolUse read hook, answer `can_use_tool`/`hook_callback`, parse
  `result`, interrupt at the TTL + grace, kill.
- `WorkerOutcome.turns` (all adapters: `None`), `WorkerCtx.{paths_allow,max_usd}`; `run_worker` reports
  engine turns.
- Tests: 10 unit/replay/e2e (fake CLI speaking the protocol) + 1 gated live test.
- Fixture: `tests/fixtures/claude-stream-json.jsonl`, a scrubbed real transcript (conformance).

## Found while building (live, s463)
The first live run denied the model's `Write`, and the model then wrote the file through Bash
(`cp`/`echo >`). Now screened: shell write targets are held to the same scope. The run-level allowlist
gate remains the backstop.

## Not done
- Protocol version pinning is by test, not by code: re-run the fixture + live test when the claude CLI moves.
- The Bash write screen is a parse, not a sandbox (see README).
