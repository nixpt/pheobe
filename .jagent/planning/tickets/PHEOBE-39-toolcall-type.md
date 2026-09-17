# PHEOBE-39 — `tool_calls[].type = "function"` on the wire (issue 12)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-39 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | foreman (s457) |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem / Resolution

See `.jagent/issues/12-*.md`. One field with a serde default; 178 tests; live-proven on a
Kaggle-GPU llama.cpp endpoint (the first strict server pheobe has met). This is also
the first pheobe run on borrowed compute — the `hermit` project's Kaggle-GPU lane baseline
(27B Q4_K_M, ~12 tok/s decode, 9 turns / 126 s for a two-file task).
