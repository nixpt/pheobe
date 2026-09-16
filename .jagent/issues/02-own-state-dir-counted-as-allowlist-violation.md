# `.pheobe/` and `done_when` byproducts are reported as allowlist violations

**Found:** 2026-09-16, foreman s456, same live run as issue 01
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
