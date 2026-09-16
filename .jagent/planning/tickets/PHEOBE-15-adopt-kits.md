# PHEOBE-15 — adopt-kit expansion + alignment

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-15 |
| **Priority** | P2 |
| **Status** | Done |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`pheobe adopt` embeds three files (claude/self-agent, opencode/host-agent,
codex README). The design names more: opencode jsonc snippet with the
sandbox mapping, claude AgentDefinition variant (already appended to the
kit), cursor `local.agents` def + hook snippet, codex Python SDK snippet,
kimi def, and an opencode `pheobe-host.md` naming inconsistency
(host-agent.md vs the design's pheobe-host.md).

## Success criteria

- [ ] `pheobe adopt <harness>` covers: claude (self + SDK snippet),
      opencode (self + host), codex (bash doc + Python SDK snippet),
      cursor (self + `local.agents` def + hooks snippet), kimi (def).
      Source-grounded per each host's reference dir; no-recursion
      enforcement noted per host (SDK disallowedTools / opencode task
      deny / cursor task-gate).
- [ ] Rename `adopt/opencode/host-agent.md` → `pheobe-host.md` (or alias)
      so `pheobe adopt opencode` and the design reference the same name.
- [ ] Each kit carries the sandbox-tier mapping table from DESIGN.md,
      trimmed to that host's column.
- [ ] `pheobe adopt` without args prints the kit index + the shared JSON
      contract reminder.

## Resolution

Merged to main (W1). adopt/README.md index + shared contract reminder;
claude sdk-snippet; opencode host-agent.md renamed to pheobe-host.md +
jsonc snippet; codex sdk-snippet (thread_start deny_all/workspace_write);
cursor def.md (local.agents + hooks + sandboxOptions); kimi def.md; bare
`pheobe adopt` prints the index. main.rs diff confined to AdoptCmd. All
kits carry their host's sandbox column + degradation rule. 9 adopt forms
verified non-empty.
