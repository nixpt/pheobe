# PHEOBE-50 — codex's workspace-write sandbox cannot commit in a linked worktree

**Filed:** 2026-09-24, foreman. Found in the live derby smoke test (the same
trivial task given to opencode, cursor, codex, commandcode and cline under
pheobe).

## Problem
The codex worker maps the `moderate` tier to codex's native
`--sandbox workspace-write`, which confines writes to the worktree. In
pheobe's worker model the **engine commits**. In a linked worktree the index
lives in the repo's shared git dir (`<repo>/.git/worktrees/<name>/index.lock`),
which is outside the worktree. codex created the file and its own check
passed, then failed with: `Git could not create the shared index.lock:
Read-only file system`. The run ended `ok: false` with zero commits.
`$CARGO_TARGET_DIR` has the same problem for any cargo task.

## Fix
`worker_codex::add_dirs`: in `workspace-write` mode, pass codex
`--add-dir <dir>` (codex-cli 0.156) for the repo's git common dir and
`$CARGO_TARGET_DIR`. Duplicates and anything already inside the worktree are
dropped. These are the same extra writable roots the moderate bwrap tier
already grants the other engines (`engine::Mounts`).

## Verified
- 2 new tests: `add_dirs_grants_the_git_store_and_target_dir_once` (pure) and
  `codex_linked_worktree_gets_its_git_common_dir_writable` (a real linked
  worktree; the shared git dir appears as `--add-dir`).
- Gates: `cargo fmt --check`, clippy `-D warnings`, full `cargo test`
  (219 + 10).
- Live: the same smoke task that failed now ends `ok: true` with 1 commit and
  its `done_when` passing (codex-cli 0.156.1).
