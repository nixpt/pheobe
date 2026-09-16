# PHEOBE-4 — opencode worker adapter (`PHEOBE_PROVIDER=opencode`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-4 |
| **Priority** | P1 |
| **Status** | Backlog |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | unassigned |
| **Dependencies** | none (PHEOBE-1/2/3 landed on main) |
| **Estimated effort** | M |

## Problem

The OpenCode integration design (DESIGN.md §"OpenCode integration") names
Surface 2: pheobe keeps the mechanical half of the loop (intake gate,
worktree, aging ladder, `done_when`, allowlist, pathspec commit, report)
and delegates the **turn loop** to opencode (`opencode run --format json`,
or a session against a running `opencode serve` via `--attach` / REST
`/api/session` + `/prompt`). Today only Surface 1 (Zen endpoint as a plain
`OpenAi` provider) is wired; Surface 2 is design-only.

## Success criteria

- [ ] A `Worker` trait exists alongside `Provider` ("one prompt in, the
      whole turn loop runs outside, one result comes back"); the loop
      dispatcher picks provider vs worker from `PHEOBE_PROVIDER`.
- [ ] `PHEOBE_PROVIDER=opencode` runs the task through `opencode run
      --agent pheobe -m "$PHEOBE_OPENCODE_MODEL" --format json` (and
      `--attach "$PHEOBE_OPENCODE_URL"` when set), with the protocol
      envelope as the prompt.
- [ ] Report normalization: engine output is prose; pheobe runs
      `pheobe verify` + mechanical gates and builds the report itself;
      engine JSON (if the last message parses) merges into
      summary/next_steps/doubts only.
- [ ] Aging ladder still enforced around a delegated engine (wall-clock
      TTL checked around the `opencode run` subprocess; kill on expiry,
      report `ttl_exceeded` as task-design failure).
- [ ] `pheobe adopt opencode` prints all three files (self-agent.md,
      host-agent.md, pheobe-host.md) + the `opencode.jsonc` snippet with
      the sandbox-tier mapping.
- [ ] Mock-able at the same seam as `Provider` (scripted subprocess or
      trait object) so tests stay network-free.
