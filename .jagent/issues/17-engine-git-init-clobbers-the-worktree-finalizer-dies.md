# An engine running `git init` in the kitchen clobbers the worktree; the finalizer then dies with no report

**Found:** 2026-09-17, foreman s500 (vega, [zorro] box), the first dispatch through the installed
Claude Code adoption kit (`adopt/claude/self-agent.md` as the subagent definition; self mode on
`llama-server`, Qwen3.5-4B Q4_K_M). The dispatcher did everything the kit says — ran `pheobe run
… --json` once, no retry, touched nothing — and got:

```
🍳 worktree: …/target-repo-pheobe-calc-divide-adopted  branch: pheobe/calc-divide-adopted
📝 bash-run byproducts (uncommitted, staged out): HEAD, README.md, __pycache__/, config, description, hooks/, info/
pheobe: command failed (exit status: 128): fatal: Invalid revision range 652f5a96…..HEAD
```

exit 1, **stdout empty**. The worktree afterwards: `.git` is a **directory** (`bare = true` in its
`config`), a bare-repo layout is spilled into the tree root (`HEAD`, `config`, `description`,
`hooks/`, `info/`, `objects/`, `refs/`), and its only history is one root commit `90a2ed0 "Add
divide(a, b) function and test"` by `Test <test@test.com>`. The model ran `git init` (and, by the
artifacts, `git init --bare .`) inside the kitchen — why is not recorded (pheobe keeps no bash
transcript in `.pheobe/`; the repo *does* carry a committer identity, and three other engines
committed fine in sibling worktrees the same hour) — and re-rooted the directory as a new
repository. The linked
worktree's `.git` *file* (`gitdir: <repo>/.git/worktrees/<name>`) was overwritten, so from the
source repo's side the branch never moved and `652f5a9` is unreachable from the kitchen's HEAD.
`worktree::commits_since(base..HEAD)` then fails and `run_task` `?`-propagates: no report.

The byproducts line is the tell — `HEAD`, `config`, `hooks/` are not bash byproducts, they are a
repository — and it was printed as informational.

**Severity:** P2 — a small model can do this on any task, and the two mechanical promises that
matter most both break at once: the worktree stops being a worktree (the parent's branch never
receives the work), and the adopter gets no report (issue 15's exit path, reached from the
finalizer instead of an adapter). The edits themselves were correct and met `done_when` by hand;
they are stranded in a directory git no longer relates to the source repo.

**Status:** Open

## Reproduction

Any engine that will run `git init` when asked to commit — a 4B model under self mode does it
unprompted. Deterministic: after `pheobe host setup`, run `git init --bare .` in the worktree,
then `pheobe host finish` / the self-mode finalizer.

## Expected behavior

- The kitchen's identity is pheobe's, not the engine's: `git init`, `git init --bare`, `git
  worktree`, `git clone` into the kitchen, and rewriting `.git` are refused by the bash tool (or
  by the bwrap tier — `.git` mounted read-only is enough on `moderate`/`strict`), the way
  `paths_allow` already refuses out-of-scope edits.
- The finalizer verifies the kitchen before diffing: `.git` is still a file whose `gitdir`
  resolves under the source repo's `.git/worktrees/`, and the base SHA is reachable. If not:
  `{"ok": false, "blocked": "kitchen re-rooted: .git is no longer the worktree pointer (engine ran
  git init?)", …}` on stdout, worktree kept, the engine's diff still recoverable.
- A bash transcript (or at least the last N commands) in `.pheobe/` so the *why* of a run like
  this is on disk next to the plan and checkpoints, instead of reconstructed from artifacts.

## Fix sketch

- `worktree.rs`: `verify_kitchen(wt, base_sha)` → `Result<(), String>` called at the top of the
  finalizer; failure becomes a `blocked` report via the same path issue 15 asks for.
- Bash tool / sandbox: deny-list `git init`, `git clone`, `git worktree` inside the kitchen.
  The stronger form — bind `.git` read-only for the engine on `moderate` — is only viable
  together with issue 16's gate change (pheobe commits whenever the tree is allowlist-clean and
  `done_when` passes, regardless of the engine's self-report); today the claude/opencode
  workers commit themselves and issue 05 counts on that.
- `.pheobe/bash.log`: append each bash-tool command + exit code during a run.
- Tests: (a) a kitchen whose `.git` was replaced by a bare init → `ok: false` with the
  `kitchen re-rooted` blocker, stdout JSON, worktree present; (b) the bash tool refuses
  `git init` with a message naming the rule.
