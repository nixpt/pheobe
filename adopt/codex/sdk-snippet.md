# pheobe — codex Python SDK snippet (worker-shaped route)

`adopt/codex/README.md` covers the bash-tool (self mode) route. This is
the Python `openai-codex` route for a host that wants codex as the
engine: one thread start with the protocol envelope as base
instructions, one turn with the task.

Source-grounded against
`/workspace/external/codex-src/sdk/python/docs/api-reference.md`:
`thread_start(*, approval_mode=..., base_instructions=None, config=None,
cwd=None, model=None, sandbox: Sandbox | None = None) -> Thread` and
`run(input, *, approval_mode=None, cwd=None, sandbox=None, ...) ->
TurnResult` (`final_response`, collected items, token usage).
`thread_resume` / `thread_fork` give resumable runs. `ApprovalMode` from
`src/openai_codex/_approval_mode.py`: `auto_review | deny_all`.

## Snippet

```python
from openai_codex import Codex, Sandbox, ApprovalMode

with Codex() as codex:
    thread = codex.thread_start(
        approval_mode=ApprovalMode.deny_all,      # headless: no questions
        sandbox=Sandbox.workspace_write,          # moderate tier
        cwd="/path/to/worktree",                  # never the source checkout
        base_instructions=protocol_envelope,      # pheobe's protocol text
    )
    result = thread.run(task_text)                # the task, stated concretely
    print(result.final_response)
```

`deny_all` is the headless no-questions mode; `auto_review` only when a
human babysits. `sandbox=` is per-turn overridable
(`thread.run(..., sandbox=)`). pheobe's mechanical gates still apply
after the turn: run `pheobe verify` + build the handoff report from
`TurnResult` — the engine's output is prose, the contract is pheobe's.

## Sandbox tier ↔ codex

Codex has the cleanest sandbox mapping of the four hosts — its own
filesystem-sandbox presets are already a three-tier ladder:

| pheobe tier | codex surface |
|---|---|
| strict | `Sandbox.workspace_write`, plus `read_only` turns when the allowlist is tight (deps vendored/locked) |
| moderate | `Sandbox.workspace_write` with `ApprovalMode.deny_all` in headless |
| free | `Sandbox.full_access` + pheobe policy invariants (safe-exec, paths_allow, no protected branches) |

Degradation rule: host can't provide the tier →
`{"ok": false, "blocked": "sandbox_unavailable"}`.
