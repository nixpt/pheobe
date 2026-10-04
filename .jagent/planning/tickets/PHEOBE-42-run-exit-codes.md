# PHEOBE-42 — `run` exit codes + JSON on every path (BREAKING)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-42 |
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

`pheobe run` now exits `0` ok / `1` ran-but-`ok:false` / `2` never-ran (intake or provisioning error), and stdout is always one JSON `HandoffReport` — errors included (`blocked: "error: …"`). Before: `ok:false` exited 0 and errors printed to stderr only. `pheobe acp` / hosts are unaffected (they call `run::run_task` directly). The pinned CLI test for an intake refusal was flipped from exit 1 to 2 in the same commit (RULES §1).
