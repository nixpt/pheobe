# codex `workspace-write` can't commit in a git worktree, and the commit gate then stands down

**Found:** 2026-09-17, foreman s500 (vega, [zorro] box), `PHEOBE_PROVIDER=codex` (codex-cli
0.154.0, `--sandbox workspace-write` = the `moderate` tier) on the same "add multiply + test"
task every other route passed. Report:

```json
"ok": false,
"commits": null,
"tests": { "passed": true, … },
"blocked": "Git staging and commit failed because the worktree Git index is on a read-only filesystem."
```

with stderr `❌ done_when failed`. The worktree afterwards: both files correctly edited, tree
dirty, `python3 -m unittest -q` green by hand, **no commit**.

Two things went wrong, one in the adapter and one in the gate:

1. **The adapter's writable root is the worktree, but the worktree's git dir is not in it.** A
   linked worktree's `.git` is a *file* — `gitdir: <repo>/.git/worktrees/<name>` — so every
   `git add`/`git commit` codex runs has to write under the parent repo's `.git`, which
   `workspace-write` mounts read-only. codex reports the failure honestly and returns
   `ok: false`.
2. **`run_task` skips pheobe's own commit gate when the engine says `ok: false`**
   (`if outcome.ok && dirty { … worktree::commit(…) }`), even though the tree is dirty,
   inside `paths_allow`, and `done_when` — which pheobe *does* still run — passes. The design
   says the mechanical gates "have the final word over the model"; here the model's
   self-assessment had the final word over the gates. The `❌ done_when failed` progress line
   is then printed because `ok` is false, although `tests.passed` is `true` (same family as
   issue 14: the label names the wrong cause).

**Severity:** P2 — the codex route cannot produce a commit on any linked worktree, which is the
only place pheobe ever runs an engine. `strict` (read-only) can't either, by design;
`free` (`danger-full-access`) can, but that is not the default and not what the tier ladder
promises.

**Status:** Open

## Reproduction

```sh
PHEOBE_PROVIDER=codex pheobe run task.json      # any task with a file edit; moderate tier
```

## Verified fix for (1) — validated on the same task, same box

Add the worktree's common git dir as a writable root. With a wrapper standing in for the adapter:

```sh
exec codex -c 'sandbox_workspace_write.writable_roots=["<repo>/.git"]' "$@"
```

`PHEOBE_CODEX_BIN=<wrapper> pheobe run task.json` → `ok: true`, commit `254e870`, 3 tests, 70 s.

## Fix sketch

- `worker_codex.rs`: resolve `git rev-parse --git-common-dir` in the worktree and pass
  `-c sandbox_workspace_write.writable_roots=["<that dir>"]` on the `moderate` tier. (The
  README's `PHEOBE_<PROVIDER>_FLAGS` promise does not hold for codex — there is no
  `PHEOBE_CODEX_FLAGS`; either add it or fix the README row.)
- `run.rs`: run the mechanical commit gate whenever the tree is dirty **and** allowlist-clean
  **and** `done_when` passes — independent of `outcome.ok`. Then `commits` is never null for
  work that provably meets the gate, and the engine's own commit becomes an optimisation, not a
  requirement. Print `❌ done_when failed` only when `tests.passed` is actually false.
- Tests: (a) adapter argv contains the `writable_roots` override with the resolved common dir;
  (b) a fake engine that edits, passes `done_when`, and reports `ok: false` still yields a
  commit and `ok: true` in the report.
