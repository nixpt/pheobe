# PHEOBE-56 — retire the `kimi` worker (cece line retirement)

**Filed:** 2026-10-15, foreman (s481), at the captain's direction. Part of squadron SQ-242.
**Blocked on:** PHEOBE-52 (bro worker), so the fleet has an engine to move to.

## Why
The installed `kimi` binary is the cece-cli fork (`uv` receipt: `directory = /workspace/projects/cece-cli`). The fleet is retiring it in favour of bro.
- In the s481 engine matrix it couldn't run at all: first an MCP startup failure, then a 402 "Insufficient Balance" from its provider.
- `PHEOBE_PROVIDER=agy` stays: the captain leaves the agy CLI as it is.

## Scope
- Remove `worker_kimi.rs`, its `REGISTRY` entry (`worker.rs` ~90), `adopt/kimi/` and `pheobe adopt kimi`, plus their tests and docs rows. `PHEOBE_PROVIDER=kimi` then errors with the known-provider list, which already names `bro`.
- CHANGELOG note; minor version bump (a removed provider).

## Done when
The full suite passes, `pheobe adopt` no longer lists kimi, and the README provider table has no kimi row.
