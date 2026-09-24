# PHEOBE-48: keep `.pheobe/` out of commits an engine makes itself

**Filed:** s463 (2026-09-23), foreman. Found by the first successful live routine dispatch (foreman-v9 57d13fe).

## Problem
pheobe's own commit path stages with the `:!/.pheobe` pathspec, but an engine that commits by itself
(`git add -A && git commit`) ignores that pathspec, so pheobe's sidecar state (`.pheobe/plan.json`) was swept in.

## Fix
- `worktree::provision` → `ensure_pheobe_excluded(wt)`: writes `/.pheobe/` into the exclude file resolved via
  `git rev-parse --git-path info/exclude` from inside the worktree. For a linked worktree that is the clone's shared
  exclude, which is clone-local and never committed. It's idempotent, and the tracked `.gitignore` is never touched.
- `stage()` keeps `:!/.pheobe` as belt-and-braces **only when pheobe's exclude line is absent**. With the path
  ignored, git rejects a pathspec naming it (`The following paths are ignored…`, exit 1). The first draft keyed this
  on `git check-ignore`, which answered "not ignored" where `git add` still refused; the run_task e2e test caught it.
- Report: any commit since base that still carries `.pheobe/` (e.g. an engine used `git add -f`) adds a `doubts`
  entry naming the sha and path. The run doesn't fail.

## Tests
src/tests/exclude.rs:
- `provision_writes_the_exclude_line_once_and_is_idempotent`
- `an_engines_add_all_commit_does_not_pick_up_pheobe_state`: fails without the fix
- `the_tracked_gitignore_is_untouched`
- `a_forced_pheobe_commit_is_named_for_the_doubts`

Full suite 221 + 10, clippy and fmt clean. Live: claude-sdk/haiku/moderate, and the engine committed itself with
`git add -A`: commit a0e8736 holds only hi.txt, `.pheobe/plan.json` stays untracked, $0.061.
