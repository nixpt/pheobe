# Worker route reports no `commits` when the engine committed itself

**Found:** 2026-09-16, foreman s457 adoption test — `PHEOBE_PROVIDER=claude`
run on a calc fixture: Claude committed `f16680b` on `pheobe/worker-claude-pow`
(verified with `git log main..branch`), done_when 6/6, `ok: true` — and the
report carried **no `commits` field at all**. Self mode on the same fixture
reported `"commits": ["0e55cf6"]` correctly.
**Severity:** P2 — `commits` is part of the contract the parent merges from;
an empty list on a successful run tells the parent nothing landed.

**Status:** Done (PHEOBE-22)

## Expected behavior

`commits` lists everything on the branch since the worktree was provisioned,
regardless of who ran `git commit` — pheobe's own commit gate or the engine
following the persona's "COMMIT with trailer" stage.

## Fix sketch

`run.rs` builds `commits` only from `worktree::commit(...)`, which runs only
when the tree is dirty after the loop. Record the base SHA at provision time
(`git rev-parse HEAD` in `worktree::provision`) and derive `commits` from
`git log --format=%h <base>..HEAD` after the gates. Keep the dirty-tree
commit as-is; it just becomes one more entry in that list.
