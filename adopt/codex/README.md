# pheobe — codex adoption (self mode)

Codex has no native subagent layer; adopt pheobe through the bash tool.

## Invocation

```bash
# task file (JSON): task, done_when, repo, paths_allow
pheobe run /tmp/pheobe-task.json --json
```

Parse the JSON report from stdout:

- `ok: true` → act on `branch` / `commits` / `tests` / `doubts`
- `ok: false` → `blocked` states the reason verbatim; don't retry silently

## Verify only (cheap gate)

```bash
pheobe verify /tmp/pheobe-task.json --worktree /path/to/worktree
```

Runs `done_when` + the paths_allow check; exit 0 = pass.

## Rules

- `done_when` is required; pheobe refuses vague asks at intake.
- pheobe cooks in its own worktree (kitchen > buckets > plain `git worktree
  add`) and never touches the source checkout; the parent merges.
- One task per run; `branch`/`doubts`/`next_steps` mean exactly that.
