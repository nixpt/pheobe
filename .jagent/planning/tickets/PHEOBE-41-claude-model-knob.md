# PHEOBE-41 — claude worker model knob

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-41 |
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

`PHEOBE_CLAUDE_MODEL` appends `--model` without replacing `PHEOBE_CLAUDE_FLAGS` (which used to be the only way, and silently dropped `--dangerously-skip-permissions`); tasks gain an optional `model` field (env wins, the `PHEOBE_SANDBOX` precedent). Plumbed through a new `WorkerCtx` + `Worker::run_with` (default = `run`, so the other six adapters are unchanged).
