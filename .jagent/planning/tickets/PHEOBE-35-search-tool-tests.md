# PHEOBE-35 — barn `search` tool tests (tools/search.rs is 14% covered)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-35 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | unassigned — claim on `scripts/pheobe-sync` |
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

- [ ] `src/tests/search.rs`: literal and regex queries over a fixture tree;
      include/exclude globs; case sensitivity; result cap / truncation shape
      (whatever the tool promises the model in its schema description);
      binary files skipped; `.gitignore`d paths honoured or not — whichever
      the tool does, pinned; a no-match query returns the documented empty
      shape, not an error; an invalid regex returns a clean error string
- [ ] the tool's schema text (`tools.rs` registration) is asserted against
      what the function actually does — the model reads that text
- [ ] `structural.rs`: the polydex-absent path (skip, not fail) and the
      stale-index path are pinned with a fake `polydex` shim on PATH (the
      pattern in `src/tests/cursor.rs` / `structint.rs` tests)
- [ ] `tools/search.rs` ≥ 80% lines, `tools/structural.rs` ≥ 70%
- [ ] no test needs a real `polydex` or `rg`

## Non-goals

Changing search semantics. If a test reveals the tool does something the
schema text does not say, file an issue and pin the current behaviour —
do not fix and test in one commit (RULES §1).
