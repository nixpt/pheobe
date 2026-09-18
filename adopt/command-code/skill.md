---
name: pheobe
description: Dispatch exactly one concrete engineering task to the pheobe coding workhorse - a mechanical done_when in, an isolated git worktree branch plus a JSON handoff report out. Use for "fix/add/verify X" jobs with a measurable exit gate; not for open-ended research, design discussion, or multi-goal asks. Triggers on "dispatch pheobe", "hand this to pheobe", "pheobe this".
argument-hint: "<one concrete task with a verification command>"
---

# pheobe — scoped coding workhorse

pheobe is a headless coding agent on PATH (`pheobe doctor` reports its
environment). It takes a task JSON, cooks in its **own** git worktree on a
fresh branch, verifies a mechanical `done_when`, and prints one JSON handoff
report. It never touches the source checkout — **the parent merges**.

`$ARGUMENTS` is the task. Run it in-session for a single task; for several
independent tasks, or one that will run long, delegate to the `pheobe`
subagent instead (same protocol, its own context window).

## 1. Task JSON

If the ask has no measurable exit gate, stop and ask for one — pheobe refuses
vague asks at intake, so inventing a `done_when` just burns a run.

```json
{
  "task": "<the task, stated concretely>",
  "done_when": { "type": "command", "run": "<the check that proves it>", "expect_exit": 0 },
  "repo": "<absolute path to the SOURCE git repo>",
  "worktree": true,
  "ttl": "45m",
  "budget": { "max_iterations": 8, "max_usd": 1.0 },
  "paths_allow": ["<only the paths this run may touch>"],
  "sandbox": "moderate",
  "push": false
}
```

Write it to `$COMMANDCODE_SCRATCHPAD/pheobe-task.json`. `done_when` also
accepts `files_exist` and `git_diff_matches`; there is no `manual`, a run
never ends on vibes. `paths_allow` is a scope contract, not a hint.

## 2. Run

```bash
pheobe run "$COMMANDCODE_SCRATCHPAD/pheobe-task.json"
```

Progress goes to stderr; the report is the only thing on stdout. Cheap gate
first if you only want the check: `pheobe verify <task.json> --worktree <dir>`
(exit 0 = pass, 1 = fail).

The engine is env-driven: `PHEOBE_BASE_URL` + `PHEOBE_MODEL` (+
`PHEOBE_API_KEY`) for an OpenAI-shaped endpoint, or
`PHEOBE_PROVIDER=opencode|claude|codex|cursor|kimi|agy` to borrow another
harness's engine. No endpoint and no provider → `pheobe` exits with
`PHEOBE_BASE_URL not set`; say so rather than running blind. `PHEOBE_SANDBOX`
= `strict|moderate|free`, `PHEOBE_MEMORY` = `none|local|host`.

## 3. Act on the report

```json
{
  "ok": true,
  "task": "...",
  "branch": "pheobe/<slug>-<id>",
  "worktree": "/path/to/worktree",
  "commits": ["<sha> <message>"],
  "tests": { "ran": "<command>", "passed": true },
  "summary": "<what changed, one paragraph>",
  "next_steps": ["<actions for the parent>"],
  "doubts": ["<unverified assumptions>"],
  "blocked": "<verbatim reason, only when ok:false>"
}
```

- `ok: true` → report `branch`, `commits`, `tests`. Then **you merge**.
- `ok: false` → relay `blocked` verbatim. Do not retry silently and do not
  "help" by editing the files yourself.
- `doubts` are pheobe's unverified assumptions — check them or hand them on.
  `next_steps` are actions for you, not narration.

Under `sandbox: moderate` the sandboxed agent cannot reach a linked
worktree's gitdir, so its own `git` calls fail and that shows up in `doubts`
even when the commit landed. Verify the branch, not the prose.

A run that fails early (no endpoint, intake refusal) leaves its worktree on
disk — `git worktree remove --force <path>` it when you're done.
