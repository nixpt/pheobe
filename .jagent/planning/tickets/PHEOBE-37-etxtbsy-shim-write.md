# PHEOBE-37 — close issue 10: ETXTBSY when parallel tests exec a just-written shim

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-37 |
| **Priority** | P3 |
| **Status** | Done |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | nixpt/cursor |
| **Dependencies** | issue 10 |
| **Estimated effort** | S |

## Problem

Adapter tests write a shell shim (`std::fs::write` + chmod) then `execve` it.
If another test thread `fork`s while the write fd is still open, the child
inherits that fd and `execve` of the shim returns ETXTBSY ("Text file busy").
One panic used to poison `PATH_LOCK` and cascade; the lock now tolerates
poisoning, but the race remains (issue 10, raised to P3).

## Success criteria

- [x] One `write_shim` helper: write+chmod a sibling tmp, rename onto the
      dest so the dest inode is never open for write
- [x] Every test shim writer (codex/claude/opencode/cursor/kimi/agy/
      structint/structural/sandbox/fmt) uses it
- [x] Worker spawn retries a short backoff on ETXTBSY (`spawn_retry` /
      `output_retry`) so a leftover busy still doesn't fail the suite
- [x] Remaining `PATH_LOCK.lock().unwrap()` (sandbox) uses `into_inner`
- [x] Issue 10 closed; CONTRIBUTING / context.md no longer tell people to
      re-run on ETXTBSY

## Resolution

`write_shim` (tmp+chmod+rename) so the dest inode is never open for write;
workers `spawn_retry` / `output_retry` on errno 26. 177 tests, clippy
`-D warnings`, fmt clean. Not pushed.

## Technical approach

- Dest-never-written (rename) is the root fix; retry is the belt.
- Do not `mod cli` this helper — it lives in `src/tests.rs` as `pub(crate)`.
- Linux/macOS ETXTBSY is errno 26 (`Error::raw_os_error()`).

## Files to modify

- `src/tests.rs`, `src/worker.rs`, adapter + sandbox + structint test shims
- issue 10, TASKS.md, CONTRIBUTING.md, `.dejavue/context.md` (+ export)

## Non-goals

- Serializing all adapter tests
- Issue 11 (`target*` walk)
