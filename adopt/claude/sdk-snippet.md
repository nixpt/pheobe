# pheobe — Claude Agent SDK subagent (self mode, Python)

The code route to the same def `adopt/claude/self-agent.md` installs on
disk: `AgentDefinition` registered on a client session. The Python SDK
ships the Claude Code CLI bundled, so this is the same substrate as the
subprocess route — pick whichever the host prefers.

Source-grounded: `AgentDefinition` in
`/workspace/external/claude-agent-sdk-python/src/claude_agent_sdk/types.py`
(fields: `description`, `prompt`, `tools`, `disallowedTools`, `model`,
`skills`, `mcpServers`, `maxTurns`).

## Snippet

```python
from claude_agent_sdk import ClaudeSDKClient, AgentDefinition

pheobe = AgentDefinition(
    description="A scoped coding workhorse. One concrete task with a "
                "mechanical done condition; works in its own worktree; "
                "hands off a JSON report. Not for open-ended asks.",
    prompt=open("adopt/claude/self-agent.md").read().split("## Agent definition")[1],
    tools=["Bash"],             # the dispatcher needs bash only
    disallowedTools=["Task"],   # pheobe subagents cannot fan out
    model="inherit",
    maxTurns=8,
)

# use: client with agents={"pheobe": pheobe}; the subagent's final message
# is the pheobe JSON report — parse and act per the Rules in self-agent.md.
```

`disallowedTools=["Task"]` enforces pheobe's no-recursive-spawning rule at
the SDK layer — same enforcement opencode's subagent permission
derivation and cursor's `task` tool gate provide at theirs.

## Sandbox tier ↔ claude

| pheobe tier | claude surface |
|---|---|
| strict | self mode only (pheobe/buckets own the namespace); SDK-side: a hooks-gated permission profile could approximate, but no namespace — fail closed |
| moderate | this def with the restricted `tools` list + the permission system (ask-by-default); SDK: permission callbacks |
| free | CLI with `--dangerously-skip-permissions` + pheobe's own policy invariants (safe-exec, paths_allow, no protected branches) |

Degradation rule: host can't provide the tier →
`{"ok": false, "blocked": "sandbox_unavailable"}`.
