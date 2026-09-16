# Adapter tests can hit ETXTBSY ("Text file busy") when run in parallel

**Found:** 2026-09-16, codex adoption exploration (read-only, sandboxed):
`cursor_*`/`codex_*` adapter tests failed once with "Text file busy" and
passed on retry. Reproduced once on the foreman box while verifying PHEOBE-25
(`worker_codex::tests::codex_failed_run_surfaces_the_stderr_tail`, panic at
`worker_codex.rs:377`; green alone and on 3 immediate re-runs).
**Status:** Done (PHEOBE-37)
**Severity:** P3 (was P4) — closed.

## Mechanism

`fake_codex` / `fake_bin` / the claude/opencode/cursor shim writers do
`std::fs::write(path, script)` then chmod and exec. `cargo test` runs test
threads in parallel; if another thread `fork`s in the window between the
write fd being open and the shim's exec, the forked child inherits the
write fd until its own exec closes it (CLOEXEC), and `execve` of the shim
returns ETXTBSY. Classic Rust-std race (rust-lang/rust#114554 family).

## Fix sketch

One shared test helper (`tests.rs`) that writes the shim and then retries
the first spawn on `ErrorKind::ExecutableFileBusy` with a short backoff —
or, cheaper, spawns the shim through `sh <path>` (the shell opens the
script for reading; no exec of the written inode). Consolidating the five
copies of the shim writer into that helper is the LOC-budget move anyway.

## Resolution

PHEOBE-37: `write_shim` writes+chmods a sibling tmp and renames onto the dest
so the dest inode is never open for write. Worker `spawn_retry` /
`output_retry` back off on errno 26. Sandbox `PATH_LOCK` uses `into_inner`.
