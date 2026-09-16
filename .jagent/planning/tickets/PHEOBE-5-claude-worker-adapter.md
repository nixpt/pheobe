# PHEOBE-5 — claude worker adapter (`PHEOBE_PROVIDER=claude`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-5 |
| **Priority** | P1 |
| **Status** | Done |
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

- [x] `PHEOBE_PROVIDER=claude` routes the turn loop to
      `claude -p <protocol+task> --output-format json <flags>` via the
      `Worker` trait (from PHEOBE-9), mockable at the same seam.
      (Resolution: `--output-format json`, not text — the mayfly text-mode
      precedent holds for shape, but json gives usage + total_cost_usd +
      a structured result; the Agent SDK docs name the same subprocess
      shape as the official escape hatch. Verified live vs claude 2.1.273.)
- [x] Env: `PHEOBE_CLAUDE_BIN` (default `claude`),
      `PHEOBE_CLAUDE_FLAGS` (default `--dangerously-skip-permissions`;
      setting it REPLACES the default entirely — e.g. `ccf-mode` appends
      nothing and expects the caller's env to carry flownet auth, same
      pattern as mayfly's ccf harness).
- [x] Aging ladder enforced around the subprocess: run_worker (PHEOBE-9)
      hard-stops before AND after the worker call — `ttl_exceeded` as
      task-design failure. The child itself is killed by the adapter's own
      subprocess timeout (`PHEOBE_CLAUDE_TIMEOUT_SECS`, default 3600, via
      the `wait-timeout` crate); set it ≤ the task ttl to make the child
      die AT the deadline. True kill-on-ladder-expiry (a shared kill switch
      between run_worker and the adapter) is a possible follow-up, not
      needed for the exit gate.
- [x] Report normalization identical to the opencode adapter: engine
      output is prose; pheobe's mechanical gates own the contract; engine
      JSON (if the last message parses) merges into
      summary/next_steps/doubts only. (Resolution: claude's result-object
      `result` string is the prose; if THAT string itself parses as a JSON
      object, it surfaces as `json_tail` and agent::run_worker merges only
      the contract keys — shared normalize_worker_outcome.)
- [x] `pheobe adopt claude` prints the `.claude/agents/pheobe.md` def AND
      an Agent SDK (Python/TS) snippet registering the same def in code.
      (Resolution: landed with the PHEOBE-15 adopt-kit merge —
      `adopt/claude/{self-agent,sdk-snippet}.md` + main.rs AdoptCmd.)
- [x] Live smoke against the fleet `claude` binary — passed: the worktree
      file was created, allowlist/commit/done_when gates all green, JSON
      report ok:true (usage.turns=1, claude tokens/cost carried from the
      result object).
