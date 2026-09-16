# PHEOBE-17 — `pheobe acp` (bro integration)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-17 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-9 (Worker trait, for the provider side), bro-cli on PATH |
| **Estimated effort** | M |

## Problem

The bro adoption route (DESIGN.md adoption matrix) is ACP:
`pheobe acp --stdio` ↔ `bro synapse dispatch --`. Not implemented; bro's
ACP client and server (`bro acp --stdio`, Unix socket mode) are the
reference implementation and peer.

## Success criteria

- [ ] `pheobe acp --stdio` speaks ACP (Agent Client Protocol) sessions:
      a dispatch message carrying a pheobe task JSON starts a run; report
      JSON returns as the final session message; streaming progress
      (stage transitions) as notifications.
- [ ] Round-trip proven against the fleet's bro binary:
      `bro synapse dispatch -- pheobe acp --stdio` with a scratch task.
- [ ] The ACP layer is a thin shell over the same `agent::run` loop —
      no loop fork.
