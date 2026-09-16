# `.pheobe/` and `done_when` byproducts are reported as allowlist violations

**Found:** 2026-09-16, foreman s456, same live run as issue 01
**Status:** Done
**Severity:** P1 — blocks any task whose repo lacks a `.gitignore` for pheobe's own artifacts
**Where:** `src/worktree.rs::check_allowlist`, `src/main.rs` (~line 156-161, the pre-commit gate)

## Steps to reproduce

Same as issue 01, in a repo with no `.gitignore`.

## Expected behavior

pheobe's own state (`.pheobe/plan.json`) is never a violation. Files created by running the task's own `done_when` command (`__pycache__/` from `python3 check.py`) are not the model's edits and should not count against `paths_allow` either — or `done_when` should be run after the allowlist snapshot.

## Actual behavior

```
"blocked": "paths outside paths_allow: alc.py, .pheobe/, __pycache__/"
```

Adding `__pycache__/` and `.pheobe/` to the repo's `.gitignore` makes both disappear (`git status --porcelain` honours ignores), which proves the gate is just reading untracked entries verbatim.

## Fix sketch

Exempt `.pheobe/` unconditionally in `check_allowlist`; either exempt untracked paths that appeared only after `done_when` ran (snapshot status before the verify command), or document that `paths_allow` must include verify byproducts. Prefer the snapshot: the allowlist is about what the *model* touched.

## Resolution

Fixed on `agent/nixp/PHEOBE-3-issues2`, with the semantics from the issue's own
sketch ("the allowlist is about what the model touched"):

- `.pheobe/` exempted unconditionally in `check_allowlist`.
- Untracked (`??`) entries outside `paths_allow` are no longer violations — they
  are returned as a separate `byproducts` list (stderr note to the run: "bash-run
  byproducts (uncommitted, staged out)"), because they are command byproducts,
  not edits. Tracked modifications outside the allowlist remain hard violations.
- The leak that made byproducts dangerous is closed at the other end: `commit()`
  now stages by pathspec (`git add -A -- <paths_allow…> :!/.pheobe`) instead of
  blind `add -A`, so `__pycache__/` and friends can never sneak into a commit.

Regression test `issue02_state_dir_exempt_and_byproducts_staged_out` (exempt +
byproduct classification + staged-out commit contents).
