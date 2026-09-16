# PHEOBE-31 — `pheobe doctor` reports compiled version vs the public channel

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-31 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W5 — release |
| **Assignee** | nixpt/cursor |
| **Dependencies** | PHEOBE-27 (public remote + tag) |
| **Estimated effort** | S |

## Problem

`pheobe doctor` never prints which binary you are running, and never
compares it to a remote. The crate is public (`nixpt/pheobe`) and
crates.io may or may not have the crate yet; `/releases/latest` 404s
because tags exist without GitHub Release objects. Renumbered from a
colliding PHEOBE-30 (foreman's publish-dispatch ticket owns that id).

## Success criteria

- [x] `pheobe doctor` prints `pheobe: <local>` as the first line.
- [x] Remote: crates.io `max_version` when the crate exists; else the
      newest `v*` git tag on `nixpt/pheobe`. Network failure is
      `channel unreachable`, exit 0.
- [x] Tests cover parse + status lines with a fake probe (no live net).

## Resolution

`src/update.rs` probes crates.io then GitHub tags. Doctor's first line
is the compiled version vs that channel. Live smoke against crates.io
`pheobe 0.2.0`: `pheobe: 0.2.0 (current)`. 144 tests, clippy `-D warnings`,
`fmt --check`. `pheobe update` remains a follow-up.

## Technical approach

- `src/update.rs` — `check(local, &dyn Probe)`; Live uses reqwest
  blocking, 2s timeout, crates.io User-Agent. Doctor prints `Status::line`.
- Do not copy bro's GitHub-asset downloader (private-repo, octostream,
  target triples). That is `pheobe update`, a later ticket.

## Files to modify

- `src/update.rs`, `src/lib.rs`, `src/main.rs`, `src/tests/update.rs`
- `README.md` doctor row, `DESIGN.md` CLI, `TASKS.md`, this ticket

## Non-goals

- `pheobe update` (cargo install wrapper)
- GitHub Release binary assets / `release-assets.yml`
- stable/nightly channels
