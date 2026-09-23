# PHEOBE-44 — `done_when`: implement `files_exist`, drop `git_diff_matches` from docs

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-44 |
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

README/DESIGN advertised `files_exist` and `git_diff_matches`; only `command` existed. `files_exist {paths}` is implemented (intake rejects an empty list; verify reports missing paths); `git_diff_matches` is no longer claimed.
