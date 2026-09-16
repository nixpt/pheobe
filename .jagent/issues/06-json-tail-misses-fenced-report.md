# `json_tail` misses a handoff report wrapped in a ```json fence

**Found:** 2026-09-16, foreman s457 adoption test — `PHEOBE_PROVIDER=claude`.
Claude ended its run with the contract JSON inside a ```json … ``` block
preceded by two sentences. `worker_claude.rs` does a bare
`serde_json::from_str(&final_text)`, which fails, so the whole prose blob
(fence and all) became `summary`, and `next_steps` / `doubts` — which Claude
had filled in correctly — were lost.
**Severity:** P2 — the worker route's only channel for `next_steps` and
`doubts` silently degrades to prose; every fenced-output model hits it.

## Expected behavior

The report's `summary`, `next_steps`, `doubts` (and `ok`/`blocked`) come from
the engine's JSON when the engine emitted one, whether bare, fenced, or
trailing prose.

## Fix sketch

One shared `extract_json_tail(text)` for all adapters (claude, opencode,
codex, cursor, kimi all normalise the same way): try bare parse; else the
last ```json fence; else the last balanced `{…}` in the text. Unit-test the
three shapes with the exact Claude output from this run.
