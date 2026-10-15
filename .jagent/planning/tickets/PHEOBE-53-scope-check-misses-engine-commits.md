# PHEOBE-53 — the scope check never sees what the engine committed

**Filed:** 2026-10-15, foreman (s481), from the engine matrix and the BRO-135 host-mode trial.
**Priority:** P1. `paths_allow` is pheobe's scope contract, and today it can be bypassed silently.

## Problem
`worktree::check_allowlist` builds its verdict from `git status --porcelain`, so it sees only **uncommitted** changes. Every engine that commits on its own (codex, cursor and agy all did in the s481 matrix) hands pheobe a clean tree, and the check passes whatever those commits touched.

Seen in the s481 matrix (toy repo, `paths_allow: ["calc.py"]`):
- `pheobe/cursor` (`2875f73`) and `pheobe/agy` (`fe93e02`) committed `.pheobe/plan.json` alongside `calc.py`;
- pheobe reported `ok: true` for both, with no violation;
- codex left `.pheobe/` alone.

The same hole lets an engine commit any out-of-scope file.

Two smaller parts of the same problem:
- **`.pheobe/` isn't excluded at provision.** After `pheobe host setup` (and in worker runs), `git status` shows `?? .pheobe/`, so an engine's `git add -A` / `git add .` stages pheobe's state. PHEOBE-48 (marked done) was meant to fix this, but on pheobe 1.2.0 its `info/exclude` line was never written: after the s481 runs, neither the toy repo's `.git/info/exclude` nor bro-cli's contains a `.pheobe` entry, and `git check-ignore .pheobe/plan.json` reports nothing in the codex worktree. So the PHEOBE-48 write path doesn't run for worker runs or `host setup`.
- **`host finish` hides the check.** `violations` is `skip_serializing_if = "Vec::is_empty"`, and byproducts are discarded (`let (violations, _byproducts)`). A host can't tell from the JSON that a scope check ran or what changed.

## Scope
1. Check the **whole run**: the union of `git diff --name-only <base>..HEAD` (base = the provision commit, already known for `commits_since`) and the porcelain status. A committed file outside `paths_allow` is a violation, `.pheobe/` included. pheobe's state must never land in a commit, whoever commits.
2. Write the `.pheobe/` exclude at **provision time**, in `worktree::provision`, so it covers host setup, self mode and every worker.
3. `host finish` / `pheobe verify` / the run report always include `changed` (files changed since base), `violations` (possibly empty) and `byproducts`.
4. Tests: an engine-style commit of an out-of-scope file is a violation; a committed `.pheobe/plan.json` is a violation; a fresh worktree's `git status` doesn't list `.pheobe/`.

## Done when
Re-running the s481 matrix task with cursor or agy either produces no `.pheobe/` commit (because it's excluded) or reports it as a violation with `ok: false`. The new tests pass.
