# PHEOBE-8 — kimi worker adapter (`PHEOBE_PROVIDER=kimi`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-8 |
| **Priority** | P3 |
| **Status** | Done |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-9 (Worker trait) |
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

- [x] `PHEOBE_PROVIDER=kimi` routes the turn loop through the Python SDK
      (`kimi-agent-sdk` pypi): Session/stream + approval handling via the
      `Worker` trait, mockable.
- [x] Config via `Config` object pointing at the fleet's endpoint
      (KIMI_* env or inline providers), so inference can be flownet /
      local / any OpenAI-shaped gateway.
- [x] Sandbox tier mapping via KAOS backends; unavailable →
      `blocked:"sandbox_unavailable"`.
- [x] Aging ladder around the session stream; approvals auto-resolved in
      headless (policy: same permit surface as mayfly's cece adapter).
- [x] `pheobe adopt kimi` prints the def/config snippet.

## Resolution (merged 2026-09-16)

`src/worker_kimi.rs`: one prompt out, one whole kimi run back —
`kimi -w <wt> -p <prompt> --print --output-format stream-json <flags>`
spawned with cwd = worktree. Default flags `--afk --yolo` (the headless
auto-approve policy from mayfly's cece adapter — this CLI is the cece
fork of Kimi Code: "cece, your next CLI agent"). `PHEOBE_KIMI_BIN`,
`PHEOBE_KIMI_FLAGS` (replace-not-append), `PHEOBE_KIMI_TIMEOUT_SECS`
(default 3600). Defensive parse: last stream-json line wins, content
parts concat, prose fallback, `{"error":...}` surfaced as a failure
(seen live: 402 Insufficient Balance reached the real fleet endpoint
and was reported clearly). Provider config via `KIMI_*` env (any
OpenAI-shaped gateway — same KIMI_* surface the kimi SDK's `Config`
reads). Sandbox: outer pheobe ladder via this worker's task.sandbox;
KAOS backend mapping stays a follow-up (no SDK python on the box by
default). 7 kimi_ tests → 95 green. Registry: `[opencode, claude,
codex, cursor, kimi]`. `pheobe adopt kimi` already printed the def.
