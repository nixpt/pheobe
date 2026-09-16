# A pre-existing `pheobe/<slug>` branch is checked out silently

**Found:** 2026-09-16, foreman s456, second run of the same task in the same repo
**Status:** Done
**Severity:** P2 — surprising, not destructive
**Where:** `src/worktree.rs` (worktree/branch creation)

## Steps to reproduce

1. Run any task → branch `pheobe/<slug-of-task>` is created.
2. Remove the worktree directory but leave the branch (or just run the same task again after committing more to `main`).
3. Run the same task again.

## Expected behavior

Either refuse (`branch pheobe/<slug> already exists — pass --branch or delete it`) or suffix (`pheobe/<slug>-2`), and in both cases start from current `main`.

## Actual behavior

`Preparing worktree (checking out 'pheobe/calc-py-has-a-bug-in-add')` — the old branch tip is checked out, so the run starts from a stale base (in the reproduction, without the `.gitignore` that had since been committed to `main`). If the worktree directory still exists but is unregistered, `git worktree add` fails outright (`missing but already registered worktree`) with no hint to `prune`.

## Fix sketch

Check `git show-ref --verify refs/heads/pheobe/<slug>` before creating; refuse or suffix; run `git worktree prune` before `add`.

## Resolution

Fixed on `agent/nixp/PHEOBE-3-issues2`: `worktree::next_free_branch()` runs
`git worktree prune` first (closes the "missing but already registered worktree"
hard failure) and takes the first free of `pheobe/<slug>`, `<slug>-2`, … `-99`;
provision() never checks out a pre-existing branch tip. Regression test
`issue03_existing_branch_suffixed_not_reused`. Live-verified: second run of the
same task got `pheobe/calc-fix-repro-2` from the current base.
