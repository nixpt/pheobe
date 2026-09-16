# PHEOBE-16 — `PHEOBE_MEMORY=none|local|host` trait swap

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-16 |
| **Priority** | P3 |
| **Status** | Backlog |
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
