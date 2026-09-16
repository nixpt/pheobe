# PHEOBE-6 — cursor worker adapter (`PHEOBE_PROVIDER=cursor`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-6 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-4 (Worker trait) |
| **Estimated effort** | M |

## Problem

Cursor has no completions endpoint ("an agent SDK, not a standalone
model-inference API" — their docs), so cursor integration is
worker-shaped only: pheobe keeps the mechanical half of the loop and
delegates the turn loop to `@cursor/sdk` / `cursor-sdk`. Cursor is also
the one host where two of pheobe's guards become *real* rather than
best-effort: `budget.max_usd` is enforceable from `agent.getUsage()`
(billed dollars, no price signal needed), and the aging injects can be
delivered mid-run via `run.steer()`.

Reference on disk: `/workspace/external/cursor-sdks` — `@cursor/sdk`
1.0.31 (ts-src), `cursor-sdk` Python 1.0.31 (whl + src; the wheel vendors
a `cursor-sdk-bridge` Node process — the Python subprocess path), doc
sources `docs-*.md`.

## Success criteria

- [ ] `PHEOBE_PROVIDER=cursor` routes the turn loop through the Python
      SDK via a small subprocess script (`python3 -c` or a shipped
      helper): `Agent.create(local=LocalAgentOptions(cwd=<worktree>),
      mode="agent")` → `agent.send(protocol+task)` → `run.wait()`, via
      the `Worker` trait, mockable at the same seam.
- [ ] Auth: `CURSOR_API_KEY` passed through.
- [ ] `budget.max_usd` enforced with real dollars: poll `agent.getUsage()`
      (or read `run.usage` + cost) around/between runs; expiry = kill +
      report as task-design failure.
- [ ] Aging ladder delivered via `run.steer()` at warn/narrow (check
      `complete_delivered` vs `revert_to_followup`); `run.cancel()` on
      hard expiry.
- [ ] Sandbox tier mapping: strict/moderate =
      `local.sandboxOptions.enabled: true`; free = hooks
      (`beforeShellExecution`/`preToolUse`) + pheobe policy invariants;
      unavailable → `blocked:"sandbox_unavailable"`.
- [ ] `pheobe adopt cursor` prints the `local.agents` pheobe subagent def
      (no-recursion: `task` disabled in the subagent's own toolset) + the
      hook config snippet.
