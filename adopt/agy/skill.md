---
name: pheobe
description: A scoped coding workhorse with no face. Dispatches one concrete engineering task with a mechanical done_when check into an isolated git worktree and returns a structured handoff report JSON. Use for targeted "fix/add/verify X" tasks, not open-ended chat.
---

# pheobe — Antigravity Skill

Dispatches an engineering task to `pheobe`. Pheobe provisions its own isolated
git worktree (or buckets runtime), runs the task loop, enforces allowlists and
commit gates, verifies `done_when`, and returns a structured JSON handoff report.

## When to use

- Concrete bugfixes, feature implementations, or refactors with a clear verification command (`cargo test`, `pytest`, `npm test`, etc.).
- When isolation is required: pheobe NEVER cooks in the main source checkout.
- NOT for exploratory research, architectural design discussions, or vague asks without a measurable exit gate.

## Workflow

### 1. Write the Task Specification

Create a temporary JSON task file (e.g. `/tmp/pheobe-task.json`):

```json
{
  "task": "<Concrete description of what to do>",
  "done_when": {
    "type": "command",
    "run": "<Command that exits 0 when complete>",
    "expect_exit": 0
  },
  "repo": "<Path to git repository root>",
  "worktree": true,
  "paths_allow": [
    "<Relative paths or patterns pheobe is allowed to touch>"
  ],
  "push": false
}
```

### 2. Execute Pheobe

Run pheobe using the command line:

```bash
pheobe run /tmp/pheobe-task.json --json
```

Or verify without running the full loop:

```bash
pheobe verify /tmp/pheobe-task.json --worktree /path/to/worktree
```

### 3. Parse the Handoff Report

Pheobe outputs a single structured JSON handoff report on stdout:

```json
{
  "ok": true,
  "task": "...",
  "branch": "pheobe/<task-slug>-<id>",
  "worktree": "/path/to/worktree",
  "commits": ["<sha message>"],
  "tests": { "ran": "<command>", "passed": true },
  "summary": "<one paragraph summary of changes>",
  "next_steps": ["<actions for the parent>"],
  "doubts": ["<unverified assumptions>"],
  "blocked": "<verbatim failure reason when ok is false>"
}
```

- **If `ok: true`**: Review `commits` and `doubts`, then merge the resulting `branch` into your target branch (the parent merges; pheobe never merges to primary branches).
- **If `ok: false`**: Inspect `blocked`. Do not retry blindly; address the root cause or adjust `paths_allow` / `done_when`.
