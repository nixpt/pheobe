# PHEOBE-10 — barn hardening: test-craft, format-on-write, checkpoints

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-10 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Assignee** | unassigned |
| **Dependencies** | none (independent of Worker trait) |
| **Estimated effort** | M |

## Problem

Three barn items are designed but not implemented in the self-mode barn
(`src/tools.rs`): structured test parsing (bro's `test.rs` pattern),
format-on-write (BRO-93), and iterate-stage checkpoints (jokersquad
`checkpoint` borrow). All are canon virtues in tool form; all are
testable without a model.

## Success criteria

- [ ] `verify` tool parses structured pass/fail for cargo/jest/pytest/go
      output (port bro's `test_run` parser: totals + failure records with
      file refs) — the raw `sh -c` path stays as fallback.
- [ ] format-on-write: after `write`/`edit`, run the file's formatter if
      installed (rustfmt/gofmt/prettier/black via bro's format.rs table);
      format failure never fails the tool call, notes a doubt instead.
- [ ] `pheobe check` subcommand (checkpoint borrow): named snapshots of
      the current dirty state via `git stash create`-style plumbing,
      never touching the working tree; the loop checkpoints at each
      plan-step boundary and restores on failed iteration instead of
      hand-editing backwards.
- [ ] `plan_tracker` validates `depends_on`: indices in range, no
      self-reference, no cycles (polydex's bounded-BFS idiom, surfaced
      `cycle_detected`), refuses to mark a step done while a `depends_on`
      predecessor is not done.
- [ ] Unit tests for all four, model-free.
