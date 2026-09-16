# PHEOBE-23 — Cursor SDK/CLI integration alignment

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-23 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W6 |
| **Assignee** | cursor (host-mode session) |
| **Dependencies** | PHEOBE-6, PHEOBE-15 |
| **Estimated effort** | S |

## Problem

PHEOBE-6 shipped a `cursor-agent` CLI worker and left sandbox mapping,
`getUsage` USD, and `run.steer()` to an `@cursor/sdk` follow-up. The
adopt kit documented only SDK `local.agents` and put no-recursion on
the wrong type: `AgentDefinition` (`options.d.ts`) has no `tools` /
`disallowedTools` — those live on `AgentOptions`. Cursor IDE/CLI loads
agents from `~/.cursor/agents/*.md`, which the kit never named. Live
`cursor-agent --help` exposes `--sandbox enabled|disabled` (the CLI
twin of `local.sandboxOptions.enabled`) and `--worktree` (a nested
isolation layer pheobe must not pass).

Reference: `/workspace/external/cursor-sdks` `@cursor/sdk` 1.0.31
(`docs-typescript-sdk.md`, `ts-src/dist/esm/agent/options.d.ts`,
`usage-types.d.ts`).

## Success criteria

- [x] `PHEOBE_SANDBOX=strict|moderate` → `cursor-agent --sandbox enabled`;
      `free` → `disabled`; unknown tier errors (never a silent default).
- [x] argv never includes `--worktree`.
- [x] Result JSON `usage.totalTokens` / `usage.total_tokens` / `usage.tokens`
      populate `WorkerOutcome.tokens`; `usage.cost.chargedCents` (cents)
      populates `usd`.
- [x] `pheobe adopt cursor` prints `def.md` + `pheobe-host.md`; kit names
      IDE `~/.cursor/agents/`, puts `disallowedTools: ["task"]` on
      `Agent.create`/`Agent.prompt` options, hooks.json `"version": 1`.
- [x] `cursor_*` tests green.

## Technical approach

- Keep the CLI worker (Worker trait is one-shot; `run.steer()` needs an
  in-process `Run` — still out of scope).
- Map the two-valued Cursor sandbox onto pheobe's three tiers the way
  DESIGN.md already collapsed them: enabled ≈ strict-shaped (no
  network); free = disabled. Document that moderate has no distinct CLI
  flag.
- Parse SDK camelCase and CLI snake_case usage keys.
- Split cursor tests into `src/tests/cursor.rs` (PHEOBE-21 LOC budget).

## Files to modify

- `src/worker_cursor.rs` — `--sandbox` + usage/cost parse
- `src/tests/cursor.rs` — argv/sandbox/usage tests
- `adopt/cursor/def.md`, `adopt/cursor/pheobe-host.md`
- `src/main.rs` AdoptCmd::Cursor
- `adopt/README.md`, `README.md`, `DESIGN.md`
- `.jagent/planning/tickets/PHEOBE-23-cursor-sdk-align.md`, `TASKS.md`

## Non-goals

- In-process `@cursor/sdk` Node/Python worker (`Agent.prompt`,
  `getUsage()` live dollars, `run.steer()` aging injects).
- Mapping moderate onto a sandboxed-plus-network mode Cursor does not
  expose.
- Recopying `~/.cursor/agents/pheobe.md` on the operator's box (the kit
  is the source; the parent recopies).

## Resolution

Grounded against `@cursor/sdk` 1.0.31 (`options.d.ts` `AgentDefinition` /
`SandboxOptions`, `usage-types.d.ts` `chargedCents`, docs §Restricting
the toolset / §Sandbox options) and live `cursor-agent --help`. CLI
worker now always passes `--sandbox enabled|disabled` from
`PHEOBE_SANDBOX` (strict/moderate share enabled — Cursor has no
sandboxed-plus-network flag); never `--worktree`. Usage parse accepts
SDK camelCase `totalTokens` + `chargedCents/100` → `usd`. Adopt kit
names the IDE `~/.cursor/agents/` surface, ships `pheobe-host.md`, and
puts `disallowedTools: ["task"]` on `AgentOptions`. 12 `cursor_*` tests
+ full `--lib` 113 green. `@cursor/sdk` in-process worker (`getUsage` /
`steer`) remains a non-goal of this ticket.
