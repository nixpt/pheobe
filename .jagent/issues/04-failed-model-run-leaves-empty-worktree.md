# A run that fails after provisioning leaves an empty worktree + branch behind

**Found:** 2026-09-16, W0 dogfood (PHEOBE-13) — two runs died on endpoint
401 after `worktree::provision` succeeded; the worktrees
(`pheobe/fix-the-bug-in-calc-py-a-3`, `-a-4`) remain on disk with no
commits, consuming the branch suffix namespace.
**Status:** Done (PHEOBE-26)

## Expected behavior

A run that exits before the model turn (endpoint/auth failure) cleans up
its own worktree, or the report says the worktree is left for inspection
with a `next_steps` cleanup hint.

## Fix sketch

In `cmd_run`, wrap provision→report in a scope; on early error (model
unreachable), `buckets worktree remove --force` the just-created worktree
unless `PHEOBE_KEEP_WORKTREE` is set. Verify with a 401-forcing test.

## Resolution

PHEOBE-26: `run_task` tears down the just-provisioned worktree on `Err`
after provision (`buckets worktree remove --force`, git fallback) unless
`PHEOBE_KEEP_WORKTREE` is set. Covered by
`run_task_missing_endpoint_tears_down_the_empty_worktree`.
