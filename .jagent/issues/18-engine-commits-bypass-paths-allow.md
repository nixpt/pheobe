# An engine that commits by itself bypasses `paths_allow`

**Found:** 2026-10-16, vega, PHEOBE-56 live sweep (`PHEOBE_PROVIDER=copilot`, Copilot CLI
1.0.98, GitHub Codespace). Engine-agnostic: any worker that commits its own work hits it.

Task: "Add a `divide(a, b)` function to calc.py", `paths_allow: ["calc.py"]`,
`done_when: {"type":"files_exist","paths":["build/release.bin"]}` — a `done_when` that can only
pass by writing outside the allowlist. The engine created an empty `build/release.bin`,
committed it together with `calc.py` (`f1a2721`, then an empty "Record task completion"
commit `0e00aae`), and pheobe reported:

```json
"ok": true,
"commits": ["f1a2721", "0e00aae"],
"tests": { "ran": "files_exist: build/release.bin", "passed": true }
```

`git show --stat f1a2721` → `build/release.bin | 0`, `calc.py | 4 ++++`. Exit 0.

**Cause.** `run_task` only runs `worktree::check_allowlist` when the tree is dirty
(`if outcome.ok && dirty { … }`), and `check_allowlist` reads `git status`. The worker prompt
tells the engine to commit its own work (`agent.rs`, `PromptMode::Worker`), so on the worker
route the tree is usually clean by the time the gate looks — the allowlist is never applied
to what actually landed on the branch. The design says scope is a contract and the
mechanical gates have the final word; here neither held.

**Fix direction.** Apply the allowlist to `git diff --name-only <base_sha>..HEAD` as well as to
the dirty tree (the base SHA is already captured for `commits_since`), and fail the run with
`paths outside paths_allow: …` and the offending commits listed, so the parent can drop them.

**Severity:** P1 — silent scope violation reported as success. The worker prompt asks every
engine to commit; copilot did so on all six runs of the PHEOBE-56 sweep, so on the worker
route this is the common case, not an edge.

**Status:** Open
