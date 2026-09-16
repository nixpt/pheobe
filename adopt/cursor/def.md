# pheobe — Cursor `local.agents` subagent def (host mode)

Cursor has no completions endpoint, so there is no self-mode surface:
the worker-shaped route is a subagent definition registered under
`local.agents: Record<string, AgentDefinition>` and spawned via the
`task` tool.

Source-grounded against `/workspace/external/cursor-sdks`:
`AgentDefinition` shape in
`ts-src/dist/esm/agent/options.d.ts` and `docs-typescript-sdk.md` =
`{ description: string (required), prompt: string (required),
model?: ModelSelection | "inherit", mcpServers? }`.

## Snippet

```typescript
const agent = await Agent.create({
  apiKey: process.env.CURSOR_API_KEY!,
  model: { id: "composer-2.5" },
  local: { cwd: "/path/to/repo" },
  agents: {
    pheobe: {
      description: "A scoped coding workhorse. One concrete task with a "
        + "mechanical done condition; works in its own worktree; hands off "
        + "a JSON report. Not for open-ended asks.",
      prompt: hostModeProtocolEnvelope,   // adopt/opencode-style host kit text
      model: "inherit",
      mcpServers: [],                     // subagents inherit parent's servers
    },
  },
});

const run = await agent.send("task: <the task, stated concretely>");
```

`prompt` carries pheobe's host-mode protocol (orient → plan → implement
→ verify → commit → handoff + the JSON report contract). The `task`
tool gates subagents, and disabling `task` on the parent prevents
subagents entirely — pheobe's no-recursive-spawning rule enforced at
the host layer, third ecosystem in a row (after claude SDK's
`disallowedTools` and opencode's task-deny derivation).

## Hooks (policy tier, file-based only)

There is no programmatic hook callback; hooks live in
`.cursor/hooks.json` (project, layered over `~/.cursor/hooks.json`) and
gate tool calls in headless runs — which otherwise auto-approve:

```json
{
  "hooks": {
    "beforeShellExecution": [{ "command": "<deny/allow script>" }],
    "preToolUse": [{ "command": "<deny/allow script>" }]
  }
}
```

## Sandbox options note

Local agents run with `local.sandboxOptions.enabled: false` by default
(no human-in-the-loop in headless runs). Enabling it constrains shell
calls: writes limited to `local.cwd` + temp + `sandbox.json` allows,
commands inside bubblewrap/seatbelt, outbound network denied by
default. If the host lacks the sandbox helper (older Linux without
bubblewrap, missing `@cursor/sdk-<os>-<arch>` binary) the SDK throws a
`ConfigurationError` naming the missing dependency — treat that as the
degradation rule, never a silent downgrade.

## Sandbox tier ↔ cursor

| pheobe tier | cursor surface |
|---|---|
| strict | `local.sandboxOptions.enabled: true` with a tight `sandbox.json` (no network, writes worktree-only) |
| moderate | `local.sandboxOptions.enabled: true` (default shape) |
| free | hooks policy tier (`beforeShellExecution` / `preToolUse` deny-patterns) + pheobe policy invariants |

Degradation rule: host can't provide the tier →
`{"ok": false, "blocked": "sandbox_unavailable"}` (a thrown
`ConfigurationError` maps to it).
