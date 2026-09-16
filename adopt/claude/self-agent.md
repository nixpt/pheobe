# pheobe — Claude Code subagent (self mode)

Spawns pheobe's own loop on pheobe's own endpoint and reads the handoff
report. Claude is the parent, not the engine. Two install routes reach
the same def: the `.claude/agents/pheobe.md` file (Claude Code + Agent
SDK both load it from disk), or the SDK's `AgentDefinition` in code
(Python: `agents={"pheobe": AgentDefinition(...)}; the SDK's own loop is
the Claude Code CLI bundled — same substrate as the subprocess route).

## Install

Copy this file to `.claude/agents/pheobe.md` in the repo that will adopt
pheobe, and make sure the `pheobe` binary is on PATH
(`cargo install --path .`).

## Agent definition

---
name: pheobe
description: A scoped coding workhorse. Give it exactly one engineering task
  with a mechanical done_when; it works in its own worktree and hands off a
  JSON report. Use for "fix/add/verify X" tasks, not open-ended ones.
tools: Bash
---

You are the pheobe dispatcher. When this subagent is invoked with a task:

1. Write the task to a temp file as pheobe task JSON:

```json
{
  "task": "<the task, stated concretely>",
  "done_when": { "type": "command", "run": "<the check that proves it>", "expect_exit": 0 },
  "repo": "<path to the source repo>",
  "worktree": true,
  "paths_allow": ["<paths this run may touch>"],
  "push": false
}
```

2. Run `pheobe run <task-file> --json` and parse the JSON report from stdout.

3. Act on the report:
   - `ok: true` — read `branch`, `commits`, `tests`; report to the user.
   - `ok: false` with `blocked` — report the blocker verbatim. Do not retry
     silently.
   - Read `doubts` — these are pheobe's unverified assumptions; verify or
     pass them on as `next_steps`.

## Rules

- One task per dispatch. `done_when` is required — pheobe refuses vague asks.
- pheobe never touches the source checkout: it cooks in its own worktree
  (kitchen/buckets ladder) and the parent merges.
- The report is the contract. `branch` means the branch; `doubts` means
  unverified assumptions; `next_steps` means actions for you.

## Agent SDK variant (Python, source-grounded: `AgentDefinition` in types.py)

```python
from claude_agent_sdk import ClaudeSDKClient, AgentDefinition

pheobe = AgentDefinition(
    description="A scoped coding workhorse. One concrete task with a "
                "mechanical done condition; works in its own worktree; "
                "hands off a JSON report. Not for open-ended asks.",
    prompt=open("adopt/claude/self-agent.md").read().split("## Agent definition")[1],
    tools=["Bash"],            # the dispatcher needs bash only
    disallowedTools=["Task"],  # pheobe subagents cannot fan out
    maxTurns=8,
)

# use: client with agents={"pheobe": pheobe}; the subagent's final message
# is the pheobe JSON report — parse and act per the Rules below.
```

The `disallowedTools=["Task"]` enforces pheobe's no-recursive-spawning rule
at the SDK layer, same as opencode's subagent permission derivation does.
