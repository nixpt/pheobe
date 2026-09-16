# PHEOBE-8 — kimi worker adapter (`PHEOBE_PROVIDER=kimi`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-8 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-4 (Worker trait) |
| **Estimated effort** | M |

## Problem

Kimi Agent SDK (Go/Node/Python) wraps Kimi CLI (Kimi Code) — the same
lineage cece-rs forked from — reusing its config, tools, skills, and MCP
servers. Fleet-native: a kimi worker adapter can point at any
OpenAI-shaped endpoint the fleet already runs, and its `kaos` sandbox
backends (BoxLite, E2B, Sprites) are the same kaos exec-layer name
cece-rs carried into the fleet.

Reference on disk: `/workspace/external/kimi-agent-sdk` (guides, go,
node, python incl. examples/python/kaos + customized-tools).

## Success criteria

- [ ] `PHEOBE_PROVIDER=kimi` routes the turn loop through the Python SDK
      (`kimi-agent-sdk` pypi): Session/stream + approval handling via the
      `Worker` trait, mockable.
- [ ] Config via `Config` object pointing at the fleet's endpoint
      (KIMI_* env or inline providers), so inference can be flownet /
      local / any OpenAI-shaped gateway.
- [ ] Sandbox tier mapping via KAOS backends; unavailable →
      `blocked:"sandbox_unavailable"`.
- [ ] Aging ladder around the session stream; approvals auto-resolved in
      headless (policy: same permit surface as mayfly's cece adapter).
- [ ] `pheobe adopt kimi` prints the def/config snippet.
