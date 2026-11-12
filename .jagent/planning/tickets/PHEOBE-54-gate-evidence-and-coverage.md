# PHEOBE-54 — gate evidence: excerpt the test summary; let done_when cover several checks

**Filed:** 2026-10-15, foreman (s481), from the BRO-135 host-mode trial.
**Priority:** P2.

## Problem
1. **`output_excerpt` is the wrong end.** On bro-cli the excerpt was the start of cargo's warning noise, cut off before any `test result:` line. The useful numbers were only in `parsed`. A human or host reading the excerpt learns nothing.
2. **One command can't cover a multi-crate scope.** The BRO-135 task allowed `bro-agent/src/` and `bro-tui/src/`, but `done_when` ran only the bro-agent lib suite. A broken bro-tui change would still get `ok: true`. The host had to run `cargo test -p bro-tui` by hand. pheobe can't infer which crates a path belongs to, but it should let the task say so.

## Scope
- The excerpt prefers the tail. Keep the last N lines, and when the parser found summary lines, include those lines verbatim (e.g. cargo's `test result:` lines, unittest's `Ran N tests` / `OK`).
- `done_when` accepts a list of commands, e.g. `{"type":"commands","run":["…","…"]}` or `done_when: [ {…}, {…} ]`, whichever fits the schema best. All must pass; the evidence lists each one.
- Docs: a note in the adopt README that `done_when` must cover every crate or package under `paths_allow`.

## Done when
- A cargo run's evidence shows the `test result:` lines.
- A two-command task fails when the second command fails, and its evidence names that command.
- Tests cover both.
