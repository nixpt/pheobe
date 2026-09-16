# PHEOBE-35 — barn `search` tool tests (tools/search.rs is 14% covered)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-35 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | nixpt/agy |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`cargo llvm-cov` (2026-09-16): `src/tools/search.rs` is **14.1% covered**
(85 of 99 lines missed, 8 of 10 functions never executed) and
`src/tools/structural.rs` 47.9%. `search` is on the engine path — the
self-mode model calls it on nearly every run to orient — and it is the only
barn tool whose behaviour is essentially unproven. A regression here makes
every self-mode run worse without failing a single test.

## Success criteria

- [x] `src/tests/search.rs`: literal and regex queries over a fixture tree;
      include/exclude globs; case sensitivity; result cap / truncation shape
      (whatever the tool promises the model in its schema description);
      binary files skipped; `.gitignore`d paths honoured or not — whichever
      the tool does, pinned; a no-match query returns the documented empty
      shape, not an error; an invalid regex returns a clean error string
- [x] the tool's schema text (`tools.rs` registration) is asserted against
      what the function actually does — the model reads that text
- [x] `structural.rs`: the polydex-absent path (skip, not fail) and the
      stale-index path are pinned with a fake `polydex` shim on PATH (the
      pattern in `src/tests/cursor.rs` / `structint.rs` tests)
- [x] `tools/search.rs` ≥ 80% lines, `tools/structural.rs` ≥ 70%
- [x] no test needs a real `polydex` or `rg`

## Non-goals

Changing search semantics. If a test reveals the tool does something the
schema text does not say, file an issue and pin the current behaviour —
do not fix and test in one commit (RULES §1).

## Resolution

- Implemented `src/tests/search.rs` (14 unit tests, 448 LOC, ≤500 budget) testing `glob` and `grep` tool executions against schemas, suffix/substring/wildcard pattern matching, directory exclusion (`.git`, `node_modules`, `target`), recursion depth limits (>12), result caps (200 for glob, 80 for grep), line truncation (`format!("{}…[{} bytes truncated]", ...)`), binary file skipping, case insensitivity, literal string matching (no regex compilation in `grep`), and path scoping/jailing.
- Discovered and filed Issue 11 (`.jagent/issues/11-search-walk-skips-any-file-named-target.md`): `walk()` in `src/tools/search.rs` checks `name.starts_with("target")` without checking `p.is_dir()`, skipping files like `target.rs`. Pinned this existing behavior in `search_walk_skips_files_starting_with_target_pinned_issue_11`.
- Implemented `src/tests/structural.rs` (4 unit tests, 287 LOC, ≤500 budget) testing structural tool registration and dispatch for `polydex` (`sym_skeleton`, `sym_callers`, `sym_impact`, `sym_affected_tests`) and `code-atlas` (`atlas_edit`) using fake shims on `PATH`, absent backend handling, stale index fallback warning (`STALE INDEX`), and exit code / stderr propagation.
- Leveraged `crate::structint::tests::PATH_LOCK` across tests modifying `PATH` to avoid races with concurrent tests.
- Verified test suite: 166 tests passing, clippy clean, fmt clean.
