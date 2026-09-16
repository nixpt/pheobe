# A tree dirty only from byproducts makes the commit gate fail with an empty reason

**Found:** 2026-09-16, foreman s457, while re-testing the claude worker
route: Claude committed its own work and left `__pycache__/` behind.
`status_dirty` counted it, the gate called `worktree::commit`, `stage`
staged nothing (byproducts are staged out), `git commit` exited 1 with
"nothing to commit" on **stdout** — which `run()` does not capture — so the
whole run died with `pheobe: command failed (exit status: 1):` and no
report. The earlier run only survived because Claude happened to `rm -rf
__pycache__` before ending.
**Severity:** P2 — a successful engine run turns into a no-report failure
depending on whether the engine tidied up.
**Status:** Done (PHEOBE-22)

## Expected behavior

Nothing stageable → no commit, no error; the report still lists the
engine's own commits (issue 05). A failing git command names its reason
whichever stream git wrote it to.

## Resolution

`worktree::commit` returns `Option<String>` (`None` when `git diff --cached
--quiet` says nothing is staged); `run()` falls back to stdout when stderr
is empty. Test: `commit_gate_is_a_no_op_when_only_byproducts_are_dirty`.
