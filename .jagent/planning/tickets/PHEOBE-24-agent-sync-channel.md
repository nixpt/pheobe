# PHEOBE-24 — agent-sync channel (`scripts/pheobe-sync`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-24 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W6 |
| **Assignee** | foreman (s457) |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

Three agents landed on pheobe main the same afternoon (PHEOBE-21 from the
captain's opencode session, PHEOBE-22 from foreman, PHEOBE-23 from cursor)
with no live channel between them — foreman found PHEOBE-23 only when the
captain said "check pheobe 23", and only avoided a ticket-number collision
by luck. nexus and bro-cli already run a repo-local sync channel for exactly
this (`scripts/nexus-sync`, `scripts/bro-sync`).

## Success criteria

- [x] `scripts/pheobe-sync` — the nexus-sync variant (bounded polling `follow`/`watch`),
      ported verbatim: `init/path/post/claim/done/ack/read/follow/watch`, `--topic`,
      identity `PHEOBE_AS` → `AGENT_NAME`, backend `PHEOBE_AGENT_SYNC_BIN` → PATH → jokersquad
- [x] channel at `<primary-checkout>/.jagent/sync/pheobe.jsonl`, shared by worktrees,
      gitignored; excluded from the crate along with `docs/SYNC.md`
- [x] `docs/SYNC.md` — protocol + keeper workflow; RULES.md §7 makes it standing discipline
- [x] smoke: `init`, `post`, `claim`, `read` from a linked worktree resolve to the primary
      checkout's file

## Non-goals

Cross-box replication (portal/bridge stays the fleet channel); a daemon.
