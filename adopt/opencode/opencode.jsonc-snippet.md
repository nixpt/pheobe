# pheobe — opencode.jsonc snippet (subagent registration)

Register pheobe in `opencode.jsonc` instead of (or alongside) the
repo-local `.opencode/agent/*.md` files (`adopt/opencode/self-agent.md`
for self mode, `adopt/opencode/pheobe-host.md` for host mode). Source
grounding: the `agent` config section maps onto opencode's
`Agent.Info` = `{ name, description, mode: "subagent"|"primary"|"all",
permission: ruleset, model?, tools?, prompt? }` (read from
the opencode source, github.com/anomalyco/opencode `packages/opencode/src/agent/`, v1.2.x).

## Snippet

```jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "agent": {
    "pheobe": {
      "description": "A scoped coding workhorse. One concrete task with a "
        + "mechanical done condition; works in its own worktree; hands off "
        + "a JSON report. Not for open-ended asks.",
      "mode": "subagent",
      "tools": { "bash": true },   // dispatcher needs bash only
      "permission": {
        "edit": "deny",
        "bash": "ask",             // ask-by-default (moderate tier)
        "task": "deny",            // no recursive spawning (rule 5)
        "external_directory": [
          "<worktree-path>",
          "/tmp"
        ]
      }
    }
  }
}
```

`task: "deny"` is belt-and-braces: opencode's subagent session
derivation already defaults `todowrite`/`task` to denied
(`deriveSubagentSessionPermission`, agent/subagent-permissions.ts), so
a pheobe subagent physically cannot fan out — no-recursion enforced at
the host layer.

## Sandbox tier ↔ opencode

| pheobe tier | opencode surface |
|---|---|
| strict | opencode cannot provide namespace isolation → in self mode only, or opencode itself run inside a `buckets run` bwrap (rare; document, don't default) |
| moderate | this agent def's own `permission` ruleset: bash/edit ask-by-default, `external_directory` allowlisted to the worktree + `/tmp` |
| free | opencode's default config policy — its deny-pattern bash rules are the same lineage as pheobe's safe-exec list, so `free` under opencode ≈ its shipped guardrails |

Degradation rule: host can't provide the tier →
`{"ok": false, "blocked": "sandbox_unavailable"}`.
