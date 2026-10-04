# PHEOBE-40 — Local-models recipe: `scripts/pheobe-local` + `docs/LOCAL_MODELS.md`

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-40 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | vega |
| **Dependencies** | none (issues 13/17 referenced, not required) |
| **Estimated effort** | S |

## Problem

Running pheobe's self mode / ACP against a local model needs a handful of engine flags that
are easy to get wrong and invisible when wrong: `--jinja` (or the tools never render), one slot
(`-np 1`, or a coding harness's prompt is rejected), the right vLLM `--tool-call-parser` for the
family (Qwen3.5 emits Qwen3-Coder XML; `hermes` silently returns `tool_calls: []`), memory
settings on a 12 GB GPU, an MTP draft when the model ships one. Every one of these was found
the hard way in s500 and then lived only in shell history.

## Success criteria

- [x] One command serves a registered model with the proven flags and prints the env pheobe
      needs (`serve NAME` → `export PHEOBE_BASE_URL=… PHEOBE_MODEL=…`); idempotent; refuses a
      port it did not open; stops the previous entry (single-tenant GPU).
- [x] One command proves the endpoint is usable (`smoke NAME`): renders tools (prompt_tokens
      delta with vs without `tools`) and returns structured `tool_calls`; exit 0 only on both.
- [x] `run NAME task.json` = serve-if-needed + `pheobe run`.
- [x] Box-specific paths live outside the repo (`~/.pheobe/models.d/NAME.env`,
      `PHEOBE_LOCAL_DIR` override); the repo carries engine knowledge only.
- [x] `docs/LOCAL_MODELS.md`: the flags and *why*, the probe, measured model notes, sizing.
- [x] Verified live on both engines: llama-server (gemma-4-12B + MTP: serve 6 s, smoke
      71/24, `run` → `ok: true` 9 turns) and vLLM (Qwen3.5-0.8B: swap-in 25 s, smoke 282/25);
      negative paths (hermes on Qwen3.5 → "structured calls: NO", bad MODEL, unknown key).

## Technical approach

- `scripts/pheobe-local` (bash, 281 lines, same shape as `scripts/pheobe-sync`): registry
  loader with a fixed key set, `engine_argv` per engine, pidfile/log/current under
  `~/.pheobe/local/`, health = `GET /v1/models`, `smoke` as an embedded python3 probe.
- No new Rust, no pheobe config file — pheobe stays environment-driven; the launcher is the
  thing that produces the environment.
- `NOTES=` in an entry is deliberately unused by the script: it is for the next person.

## Files to modify

- `scripts/pheobe-local` — new
- `docs/LOCAL_MODELS.md` — new
- `README.md` — one pointer line under Configuration
- `.jagent/planning/TASKS.md` — row

## Non-goals

- zorro serve as an engine (`ZORRO-020` first — `smoke` would report `renders tools: NO`).
- Multi-tenant GPU scheduling; the script assumes one engine at a time.
- Driving the worker adapters' models (opencode/claude/codex bring their own); documented,
  not automated.
