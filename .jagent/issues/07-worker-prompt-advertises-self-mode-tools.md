# Worker route reuses the self-loop prompt, which names tools the engine does not have

**Found:** 2026-09-16, foreman s457 adoption test — `PHEOBE_PROVIDER=claude`.
Claude's final message opened with "The `handoff` tool isn't exposed in this
session, so the report follows the contract verbatim". `agent::build_prompt`
is shared by `agent::run` (self loop, where `plan_tracker`, `verify`,
`handoff` are real tool-calls) and `agent::run_worker`, where the engine is
an external harness with its own tools and none of those names. Rules say
"Plan first with plan_tracker", "Verify with the verify tool", "END ONLY by
calling handoff".
**Severity:** P2 — Claude reasoned around it; a weaker engine will stall
looking for a tool that does not exist, or never end the run.

## Expected behavior

The worker prompt describes the same six stages in terms the engine can act
on: write `.pheobe/plan.json`, run the `done_when` command (or
`pheobe verify <task>` when the binary is on PATH), commit with the
`Pheobe-Task:` trailer, and END by printing the report JSON as the final
message — the same protocol the host-mode kits already spell out.

## Fix sketch

Split `build_prompt` into a shared body plus a mode-specific "how to plan /
verify / end" section; `run_worker` renders the worker variant. The
opencode host kit (`adopt/opencode/pheobe-host.md`) is the wording to reuse.
