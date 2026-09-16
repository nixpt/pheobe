# pheobe work and discussion channel

Use `scripts/pheobe-sync` for pheobe task coordination, progress, blockers, handoffs,
and design discussion. It wraps the existing jokersquad `agent-sync` protocol;
it does not introduce another message format or daemon.

```bash
export PHEOBE_AS=nixpt/cursor  # use your own contributor token
# Codex: PHEOBE_AS=nixpt/codex
scripts/pheobe-sync read --tail 30
scripts/pheobe-sync --topic PHEOBE-24 post 'Proposing prior-art HEAD refresh next.'
scripts/pheobe-sync claim .jagent/planning/tickets/PHEOBE-24.md 'Starting docs refresh.'
scripts/pheobe-sync done .jagent/planning/tickets/PHEOBE-24.md 'Landed; HEADs updated.'
scripts/pheobe-sync ack RECORD_ID 'Read; incorporating the feedback.'
scripts/pheobe-sync follow --tail 10 --interval 2
```

`claim` announces intended ownership; it is **not an exclusive lock** and does
not assign a ticket. Read recent claims first and resolve overlaps with the
other agent. `done` records completion or release; state why if work is paused.
`ack` references a record ID. There are no private messages or read receipts.
Topics label records in the same channel; they do not create separate files or
filter `read`. Default topic: `pheobe`.

## One channel per clone, shared by its worktrees

`scripts/pheobe-sync path` prints the authoritative file:

```text
<primary-checkout>/.jagent/sync/pheobe.jsonl
```

The wrapper resolves the primary checkout from its own Git worktree, even when
called from another directory. Linked worktrees of that clone use the same log.
Separate clones and other boxes have independent logs; this is local coordination,
not network replication. A worktree sandbox needs access to the primary
checkout's channel to write there; do not silently create a second channel when
that access is unavailable.

The directory is gitignored. Channel messages and lock files do not belong in
commits or PRs.

## Protocol and dependencies

The upstream tool appends JSONL records with `id`, `from`, `ts`, `type`, `body`,
`topic`, `repo`, `cwd`, `resource`, and `reply_to`. It serializes appends using
`flock`; this wrapper requires both `flock` and `jq`. Claims are ordinary records,
not OS locks on the claimed resource. `read --raw` exposes the upstream JSONL.

Backend lookup is `PHEOBE_AGENT_SYNC_BIN` (explicit executable path), then
`agent-sync` on PATH, then `/workspace/projects/jokersquad/bin/agent-sync` on this
box. Install the fleet tool or set that override on another machine. The wrapper
always supplies an explicit channel file, so ambient `AGENT_SYNC_FILE` and
`AGENT_COMMS_DIR` cannot redirect pheobe traffic. `--file` and `--name` are not
wrapper options. `PHEOBE_AS` becomes the sender; otherwise upstream uses
`AGENT_NAME`, falling back to the local user.

`init` creates an empty channel without truncating existing records; posts also
create it if absent. `follow` (and its `watch` alias) polls a bounded `read`
snapshot and emits only when the snapshot changes. It runs until interrupted;
`PHEOBE_SYNC_INTERVAL` sets the default interval in seconds. This avoids handing
a long-lived `tail -f` pipe to a session that may stop being polled. To use the
backend directly:

```bash
agent-sync --file "$(scripts/pheobe-sync path)" --topic PHEOBE-24 read --tail 20
```

## Keeper workflow

On arrival, read the recent channel alongside the planning board. Post a claim
before overlapping edits, then updates that name the resource, result, or blocker.
At handoff, post what finished and what remains; acknowledge messages that need
an explicit response. Keep accepted decisions in `.dejavue/`, task status in
`.jagent/planning/`.

The channel makes discussion visible; it does not replace those durable records.
