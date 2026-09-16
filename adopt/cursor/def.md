# pheobe — Cursor adoption kit (host mode)

Cursor has no completions endpoint ("an agent SDK, not a standalone
model-inference API" — `docs-typescript-sdk.md`). There is no self-mode
surface that talks to a Cursor model URL. Three install surfaces, one
JSON report contract:

| surface | who drives | install |
|---|---|---|
| Cursor IDE / `cursor-agent` session | this host's model | copy `pheobe-host.md` to `~/.cursor/agents/pheobe.md` (user) or `.cursor/agents/pheobe.md` (project). Optional: `~/.cursor/skills/pheobe/SKILL.md` so the *current* session runs the protocol without a Task spawn |
| `@cursor/sdk` `Agent.create` | the SDK local loop | `local.agents` def below + `disallowedTools` on **AgentOptions**, not on `AgentDefinition` |
| `PHEOBE_PROVIDER=cursor` | pheobe's mechanical loop, `cursor-agent` as the engine | no kit file; the worker adapter is the kit |

`pheobe adopt cursor` prints this file, then `pheobe-host.md`.

Source-grounded against `/workspace/external/cursor-sdks` `@cursor/sdk`
1.0.31: `AgentDefinition` in `ts-src/dist/esm/agent/options.d.ts` =
`{ description, prompt, model?: ModelSelection \| "inherit", mcpServers? }`.
`tools` / `disallowedTools` live on `AgentOptions` (parent create/resume),
not on `AgentDefinition`. Live CLI: `cursor-agent --help`
(`--sandbox enabled\|disabled`, `--yolo` = `--force`, `--worktree` is
Cursor's *own* isolation — never pass it; pheobe already provisioned cwd).

## 1. IDE / CLI host (`~/.cursor/agents/`)

Copy `adopt/cursor/pheobe-host.md` to `~/.cursor/agents/pheobe.md`.
Cursor loads user-level agents from that directory (project copy at
`.cursor/agents/` wins on name collision). The body is the system prompt;
frontmatter is `name` + `description` only.

A session that is *already* running that prompt must not also Task-spawn
the twin. No-recursion is prompt discipline here: the IDE agent file has
no `disallowedTools` field.

## 2. SDK host (`Agent.create` / `Agent.prompt`)

`AgentDefinition` cannot deny `task`. Put `disallowedTools: ["task"]` on
the **create options** when pheobe *is* the agent (systemPrompt route).
When pheobe is only a named subagent, the parent must keep `task` or it
cannot spawn pheobe; SDK nesting then allows that first-level subagent
to spawn further unless the prompt forbids it. `mcpServers` on
`AgentDefinition` is accepted for forward-compat and ignored (subagents
inherit the parent's MCP servers; inline configs throw
`ConfigurationError`).

```typescript
import { Agent } from "@cursor/sdk";

// pheobe IS the agent (preferred SDK host mode): no nested task.
const result = await Agent.prompt("task: <the task, stated concretely>", {
  apiKey: process.env.CURSOR_API_KEY!,
  model: { id: "composer-2.5" },
  systemPrompt: hostModeProtocolEnvelope, // adopt/cursor/pheobe-host.md body
  disallowedTools: ["task"],              // AgentOptions, not AgentDefinition
  local: {
    cwd: "/path/to/pheobe-worktree",      // pheobe-provisioned worktree
    sandboxOptions: { enabled: true },    // moderate/strict; omit for free
  },
});

// pheobe as a named subagent the parent may spawn:
const agent = await Agent.create({
  apiKey: process.env.CURSOR_API_KEY!,
  model: { id: "composer-2.5" },
  local: { cwd: "/path/to/repo" },
  agents: {
    pheobe: {
      description: "A scoped coding workhorse. One concrete task with a "
        + "mechanical done condition; works in its own worktree; hands off "
        + "a JSON report. Not for open-ended asks.",
      prompt: hostModeProtocolEnvelope,
      model: "inherit",
      mcpServers: [],
    },
  },
});
```

`local.sandboxOptions.enabled: true` is the SDK twin of
`cursor-agent --sandbox enabled`. Missing helper binary →
`ConfigurationError` → `{"ok": false, "blocked": "sandbox_unavailable"}`.
Never silent-downgrade. Do not pass Cursor's `--worktree` / SDK cloud
`autoCreatePR` as a substitute for pheobe's worktree ladder.

## 3. Worker (`PHEOBE_PROVIDER=cursor`)

`cursor-agent -p --output-format json --sandbox <enabled|disabled> --yolo --trust <prompt>`
with cwd = the pheobe worktree. `--yolo` is the CLI name for `--force`.
`PHEOBE_CURSOR_FLAGS` replaces `--yolo --trust` entirely; `--sandbox`
stays pheobe policy (`PHEOBE_SANDBOX`).

## Hooks (policy tier, file-based only)

No programmatic hook callback. Project `.cursor/hooks.json` layers over
`~/.cursor/hooks.json`. Schema version 1:

```json
{
  "version": 1,
  "hooks": {
    "beforeShellExecution": [{ "command": "<deny/allow script>" }],
    "preToolUse": [{ "command": "<deny/allow script>", "matcher": "Task" }]
  }
}
```

Headless SDK/CLI runs auto-approve tool calls unless hooks or sandbox
gate them.

## Sandbox tier ↔ cursor

| pheobe tier | SDK | `cursor-agent --sandbox` |
|---|---|---|
| strict | `local.sandboxOptions.enabled: true` (no network; writes cwd/temp/`sandbox.json`) | `enabled` |
| moderate | same `enabled: true` — Cursor has no sandboxed-plus-network flag | `enabled` |
| free | hooks policy + pheobe invariants (`paths_allow`, no protected branches) | `disabled` |

Cursor's enabled sandbox is pheobe-strict-shaped (outbound network
denied unless `.cursor/sandbox.json` / `~/.cursor/sandbox.json` allows
hosts). Moderate tasks that must reach a package registry may need
`PHEOBE_SANDBOX=free` or an explicit sandbox.json allowlist — record
that in `doubts`, do not silently disable the sandbox.

Degradation: host can't provide the requested tier →
`{"ok": false, "blocked": "sandbox_unavailable"}`.
