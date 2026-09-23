# PHEOBE-43 — claude worker: ttl-bounded timeout + sandbox

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-43 |
| **Priority** | P1 |
| **Status** | Done (in review) |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | foreman (s463) |
| **Dependencies** | none |
| **Estimated effort** | S |

## Why

The foreman's tiered dispatch (foreman-v9 FMN-4: read → mayfly, routine → pheobe,
lead → foreman) needs pheobe to be dispatchable by a program: a model knob, exit codes a
dispatcher can branch on, and a claude worker that is actually bounded.

## Resolution

Effective subprocess timeout = min(`PHEOBE_CLAUDE_TIMEOUT_SECS`, task ttl), so the child dies AT the ttl. The resolved sandbox tier now wraps the whole claude run: `moderate` = bwrap (network shared; writes: worktree, repo git store, `~/.claude`, `$CARGO_TARGET_DIR`; `$HOME` + repo parent read-only), `strict` refused with a clear error (the CLI needs the network), `free` plain. Direct `run()` callers with no tier stay unsandboxed. Live-proven s463: a real claude haiku run under moderate bwrap created the file, committed with the `Pheobe-Task` trailer, `ok:true`, exit 0 — after a first run found the git store must be writable (the worker prompt tells the engine to commit).
