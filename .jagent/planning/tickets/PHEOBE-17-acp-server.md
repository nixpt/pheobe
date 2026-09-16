# PHEOBE-17 — `pheobe acp` (bro integration)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-17 |
| **Priority** | P3 |
| **Status** | Done |
| **Assignee** | nixp |
| **Dependencies** | PHEOBE-9 (Worker trait, for the provider side), bro-cli on PATH |
| **Estimated effort** | M |

## Problem

The bro adoption route (DESIGN.md adoption matrix) is ACP:
`pheobe acp --stdio` ↔ `bro synapse dispatch --`. Not implemented; bro's
ACP client and server (`bro acp --stdio`, Unix socket mode) are the
reference implementation and peer.

## Success criteria

- [x] `pheobe acp --stdio` speaks ACP (Agent Client Protocol) sessions:
      a dispatch message carrying a pheobe task JSON starts a run; report
      JSON returns as the final session message; streaming progress
      (stage transitions) as notifications.
- [x] Round-trip proven against the fleet's bro binary:
      `bro synapse dispatch -- pheobe acp --stdio` with a scratch task.
- [x] The ACP layer is a thin shell over the same `agent::run` loop —
      no loop fork.

## Resolution

`src/acp.rs`: an ACP `Agent` over stdio, transport patterned on bro's
`soul/acp_server::run` — `Lines` built from stdin/stdout (NOT the crate's
`Stdio`, which parks forever on client stdin close), incoming-stream end
treated as hangup: answer what was received, flush, leave.

- initialize → capabilities + `_meta.cwd`
- session/new → stores session id → cwd in a `SessionStore`, responds
  with `pheobe-{nanos:x}` session id
- session/prompt → spawns a connection task (never blocks the dispatch
  loop handler); `spawn_blocking` runs `task::load_from_str` +
  `run::run_task` — the SAME pipeline `pheobe run` uses, no loop fork;
  stage transitions stream back through an mpsc channel as
  `AgentMessageChunk` notifications; the handoff report JSON (compact
  serde) is the final session message; `PromptResponse(EndTurn)` on
  success, `Refusal` + error text chunk on failure. Unparseable
  task JSON = clean Refusal, not a hang.

`src/run.rs` (new): `run_task(task, branch, progress)` extracted from
`cmd_run` — the whole pipeline (intake gates, worktree, learn session,
briefs, provider turn via PHEOBE_PROVIDER registry, gates, report, push).
CLI keeps `report::emit` printing; ACP uses the progress callback for
notifications and the report as the final message.

`src/main.rs`: `Cmd::Acp { stdio }` (`--stdio` accepted for CLI parity
with `bro acp --stdio`), `cmd_acp` = `learn::init()` + tokio rt +
`acp::serve_stdio()`. `cmd_run` now delegates to `run::run_task`.
`src/task.rs`: `load_from_str` (ACP dispatch prompt carries task JSON
inline, no file).

Deps: `agent-client-protocol` 1.2 (resolved 1.3.0, unstable feature),
`tokio` (rt-multi-thread, macros, io-util, io-std, sync, time),
`tokio-util` (compat), `futures`.

Tests (100 green, 0 warnings): in-memory duplex wire tests (ByteStreams
over tokio::io::duplex, driven by the crate's own `Client` role):
initialize + session/new round-trip; prompt with garbage task → error
chunk + `Refusal` (the wire contract bro relies on to surface a failure
instead of hanging); plus `extract_text`/`new_session_id` unit tests.

Live round-trip smoke (both proven with a one-shot mock OpenAI endpoint
on 127.0.0.1 answering POST /chat/completions with a `handoff`
tool-call, PHEOBE_PROVIDER unset → built-in loop):
- `pheobe run <task>` → `{"ok":true,...}` report
- `bro synapse dispatch --task <task JSON> -- pheobe acp --stdio` →
  🍳 worktree progress notification streamed + the identical report
  JSON returned as the dispatch response.

Bug found by the smoke (fixed here): a successful handoff with zero
source edits used to look "dirty" purely because `plan::save` wrote
`.pheobe/plan.json` — then `commit()` staged nothing (`stage` always
excludes `.pheobe`) and `git commit` died with a bare exit-1
"nothing to commit". Fix: `worktree::status_dirty` now ignores
`.pheobe/` sidecar entries; regression test
`sidecar_pheobe_dir_alone_is_not_dirty`.
