# PHEOBE-5 — claude worker adapter (`PHEOBE_PROVIDER=claude`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-5 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-4 (Worker trait lands there first) |
| **Estimated effort** | S (after PHEOBE-4) |

## Problem

Claude integration Surface 2 (DESIGN.md §"Claude integration"): pheobe
keeps the mechanical half of the loop and delegates the turn loop to
`claude -p <protocol envelope> --output-format json`. mayfly already
proves the claude adapter shape; the Agent SDK docs name the same
subprocess shape as the official escape hatch for other languages.

Reference on disk: `claude` CLI 2.1.273 (this box),
`/workspace/external/claude-agent-sdk-python` (bundles the same CLI — the
SDK is the CLI wrapped), `claude-agent-sdk-typescript` (0.3.273),
`ant` 1.30.0 in `~/.local/bin`.

## Success criteria

- [ ] `PHEOBE_PROVIDER=claude` routes the turn loop to
      `claude -p <protocol+task> --output-format text <flags>` via the
      `Worker` trait (from PHEOBE-4), mockable at the same seam.
- [ ] Env: `PHEOBE_CLAUDE_BIN` (default `claude`),
      `PHEOBE_CLAUDE_FLAGS` (default `--dangerously-skip-permissions`).
- [ ] Aging ladder enforced around the subprocess: TTL expiry kills the
      child, reports `ttl_exceeded` as task-design failure.
- [ ] Report normalization identical to the opencode adapter: engine
      output is prose; pheobe's mechanical gates own the contract; engine
      JSON (if the last message parses) merges into
      summary/next_steps/doubts only.
- [ ] `pheobe adopt claude` prints the `.claude/agents/pheobe.md` def AND
      an Agent SDK (Python/TS) snippet registering the same def in code.
- [ ] Live smoke against the fleet `claude` binary (or `ccf` env) — same
      script mayfly's `smoke-harness.sh claude` uses.
