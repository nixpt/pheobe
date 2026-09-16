# PHEOBE-26 — host-mode supervisor (`pheobe host setup` / `finish`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-26 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W6 |
| **Assignee** | nixpt/cursor |
| **Dependencies** | PHEOBE-15 (host kits), issue 04 |
| **Estimated effort** | S |

## Problem

Self mode owns intake, worktree, aging, allowlist, commit, and report.
Host mode owns almost none of that: the kit is a prompt, `pheobe verify`
is optional, and a `pheobe run` that dies after `provision` (endpoint
401) leaves an empty worktree + branch (issue 04). The host LLM can
ignore `done_when`. pheobe should own the kitchen in both modes.

## Success criteria

- [x] `pheobe host setup <task.json>`: fuzziness gate, sandbox intake,
      provision a worktree, seed `.pheobe/plan.json`, print JSON
      `{ok, worktree, branch, task}` on stdout.
- [x] `pheobe host finish <task.json> [--worktree PATH]`: `done_when` +
      `paths_allow` in that worktree; JSON `{ok, tests, violations}` on
      stdout; exit 0/1.
- [x] `run_task` tears down a just-provisioned worktree when the model
      turn never starts (missing endpoint / spawn error), unless
      `PHEOBE_KEEP_WORKTREE` is set. Issue 04 closed.
- [x] Tests: setup+finish roundtrip; vague setup refused; failed
      `run_task` leaves no leftover worktree.

## Resolution

`pheobe host setup` / `finish` own the host kitchen. `run_task` tears down
the just-provisioned worktree on post-provision `Err` unless
`PHEOBE_KEEP_WORKTREE` is set. 117 tests green (`cargo test --offline`,
`clippy -D warnings`, `fmt --check`).

## Technical approach

- `worktree::teardown` — `buckets worktree remove --force` when buckets
  provisioned, else `git worktree remove --force` + `branch -D`.
- `src/host.rs` — setup/finish; CLI subcommand in `main.rs`.
- `run::run_task` wraps the post-provision body; `Err` → teardown.

## Files to modify

- `src/worktree.rs`, `src/run.rs`, `src/host.rs`, `src/main.rs`, `src/lib.rs`
- `src/tests/host.rs`
- `.jagent/issues/04-…`, `TASKS.md`, this ticket
- `DESIGN.md` CLI line; host kits mention setup/finish

## Non-goals

- Host LLM inside pheobe (still the adopter's model)
- `pheobe adopt` actually installing files (separate renderer ticket)
- Mid-run Worker steer / `getUsage`
- Aging injects into a host session
