# PHEOBE-7 — codex worker adapter (`PHEOBE_PROVIDER=codex`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-7 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-9 (Worker trait) |
| **Estimated effort** | S (after PHEOBE-4) |

## Problem

Codex integration Surface 2 (DESIGN.md §"Codex + Kimi integration"): the
cleanest sandbox mapping of all four hosts — codex's own
`Sandbox.read_only | workspace_write | full_access` presets are already a
three-tier ladder, and `ApprovalMode.deny_all` is the headless
no-questions mode. Thread/turn model maps 1:1 onto pheobe's loop.

Reference on disk: `/workspace/external/codex-src/sdk/python` (README,
docs/{getting-started,api-reference,faq}.md, examples, src).

## Success criteria

- [ ] `PHEOBE_PROVIDER=codex` routes the turn loop through a Python
      subprocess: `thread_start(sandbox=, approval_mode=deny_all,
      cwd=<worktree>, base_instructions=<protocol envelope>)` →
      `thread.run(task)` → TurnResult, via the `Worker` trait, mockable.
- [ ] Sandbox tier mapping: strict = `read_only`-with-allowlist (or
      `workspace_write` + tight paths_allow), moderate =
      `workspace_write`, free = `full_access` + policy invariants.
- [ ] Token usage from TurnResult feeds the budget; aging ladder enforced
      around the thread (kill/interrupt on expiry → `ttl_exceeded`).
- [ ] `pheobe adopt codex` prints the bash-tool dispatch doc (exists) plus
      a Python SDK snippet using the same task JSON.
- [ ] Live smoke against the fleet codex binary / existing codex auth.
