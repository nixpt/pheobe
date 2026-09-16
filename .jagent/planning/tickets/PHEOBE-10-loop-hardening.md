# PHEOBE-10 — barn hardening: test-craft, format-on-write, checkpoints

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-10 |
| **Priority** | P1 |
| **Status** | Done |
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

- [x] `verify` tool parses structured pass/fail for cargo/jest/pytest/go
      output (port bro's `test_run` parser: totals + failure records with
      file refs) — the raw `sh -c` path stays as fallback.
- [x] format-on-write: after `write`/`edit`, run the file's formatter if
      installed (rustfmt/gofmt/prettier/black via bro's format.rs table);
      format failure never fails the tool call, notes a doubt instead.
- [x] `pheobe check` subcommand (checkpoint borrow): named snapshots of
      the current dirty state via `git stash create`-style plumbing,
      never touching the working tree; the loop checkpoints at each
      plan-step boundary and restores on failed iteration instead of
      hand-editing backwards.
- [x] `plan_tracker` validates `depends_on`: indices in range, no
      self-reference, no cycles (polydex's bounded-BFS idiom, surfaced
      `cycle_detected`), refuses to mark a step done while a `depends_on`
      predecessor is not done.
- [x] Unit tests for all four, model-free.

## Resolution

Implemented on `agent/nixp/PHEOBE-10-barn` (2026-09-16), all four items,
29 tests green (16 pre-existing + 13 new), build warning-free.

- `src/testparse.rs` — bro's test.rs pattern ported as hand-rolled line
  scanners (no regex dep; pheobe keeps its dep set small). Parsers: cargo
  (`test result:` summary + run-lines/`failures:` section + `panicked at`
  file refs), jest (`Tests:` line + `●` failures), pytest (independent
  count tokens — summary order varies by outcome — + `FAILED ref - msg`),
  go (`--- PASS/FAIL:` markers with file refs from the log window, falling
  back to package-level `ok`/`FAIL` lines). Verify tool result gains
  `structured`; `TestEvidence` gains `parsed`.
- `src/fmt.rs` — bro's format.rs table trimmed to rustfmt/gofmt/black/
  prettier (prettier resolved to `<wt>/node_modules/.bin/prettier`, a
  project dep not a global). `PHEOBE_FORMAT=off` kill-switch (BRO-93).
  Formatter failure = doubt note in the tool result, never an error;
  missing binary = silent skip. Wired into write + edit handlers.
- `src/checkpoint.rs` — jokersquad `checkpoint` discipline: `create` runs
  `git stash create` + `stash store` (never touches the working tree),
  registry `.pheobe/checkpoints.json` upsert by name; `restore` = stash
  apply (the one tree-touching verb); `prune` keeps the newest 5 and
  drops their stash refs best-effort. `pheobe check create|list|restore|prune`
  subcommand (operates on cwd). The loop checkpoints at plan-step
  boundaries (plan_tracker upsert, best-effort); restore-on-failed-
  iteration stays a manual `pheobe check restore` — the loop has no
  mechanical notion of a failed iteration to key off (noted for the
  follow-up ticket if the host mode wants it automatic).

## Resolution

Merged to main (W1). src/testparse.rs (cargo/jest/pytest/go line
scanners, no regex dep), src/fmt.rs (format-on-write table +
PHEOBE_FORMAT=off), src/checkpoint.rs + `pheobe check create/list/
restore/prune` (git-stash plumbing, never touches the tree on create),
plan_tracker depends_on validation (bounded-DFS cycle detection,
predecessor-done refusal). 13 new tests, 29 on that branch. Loop wiring:
checkpoint-on-step-done (restore stays manual — no mechanical failed-
iteration signal exists; documented).
