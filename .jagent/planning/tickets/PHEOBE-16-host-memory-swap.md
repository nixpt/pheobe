# PHEOBE-16 — `PHEOBE_MEMORY=none|local|host` trait swap

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-16 |
| **Priority** | P3 |
| **Status** | Done |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-10 (loop hardening touches the same files) |
| **Estimated effort** | S |

## Problem

The learn store (PHEOBE-2-era borrow) is local-JSONL only. The design
commits to `PHEOBE_MEMORY=none|local|host` (default local) — `host`
routes to the adopting harness's own memory surface; `none` keeps the
run memoryless. The store interface must become a trait so host mode is
a swap, not a fork.

## Success criteria

- [ ] `src/learn.rs` split behind a `MemoryStore` trait: `session_begin/
      end`, `log_event`, `store_nudge`, `nudges_for` — `JsonlStore` is
      the current implementation.
- [ ] `none` = the `NullStore`; host = delegated (first target: joker's
      `joker_store_fact`/`joker_recall_facts` MCP surface, invoked as a
      tool call when on a joker box — subprocess or best-effort, absent
      = fall back to local with a doubt note).
- [ ] `PHEOBE_MEMORY` env selects at run start; host-mode kits declare
      the mapping (DESIGN.md §"Borrowed: joker learning" already
      specifies the semantics).
- [ ] Existing learn tests pass unchanged against the trait.

## Resolution (merged 2026-09-16)

`src/memory.rs`: `MemoryStore` trait (`session_begin/end`, `log_event`,
`store_nudge`, `nudges_for`) + three impls. `JsonlStore` = the prior
learning JSONL (moved verbatim). `NullStore` = PHEOBE_MEMORY=none no-ops.
`HostStore` = joker-mcp MCP stdio delegation: binary probe via PATH
resolvability (joker-mcp `--help` exits 1 as an MCP server won't run
that way), then a real MCP handshake (initialize ack → notifications/
initialized → tools/call) with a per-line response reader and 5s
timeout; store → joker_store_fact, recall → joker_recall_facts filtered
to `nudge[<repo>]`. Any failure → local fallback + ⚠ doubt note.

`learn.rs` free functions now delegate to `memory::current()`; `main.rs`
calls `learn::init()` at run start. Existing learn tests pass unchanged
(only env serialization added via a shared `env_lock`). 88 tests green.
Live proof: `PHEOBE_MEMORY=host pheobe learn nudge/nudges` round-trips
through the real joker-mcp on this box.
