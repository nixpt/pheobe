# PHEOBE-25 — AGY worker adapter and adoption kit

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-25 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | M1 — adoptable / W6 — fidelity |
| **Assignee** | nixpt/agy |
| **Dependencies** | PHEOBE-9 (Worker trait), PHEOBE-15 (adopt kits) |
| **Estimated effort** | M |

## Problem

Antigravity (`agy`) is the fleet's primary developer agent platform (CLI, IDE, subagents, and Python SDK). While Pheobe has worker adapters and adoption kits for Claude, Cursor, Codex, OpenCode, and Kimi, it lacks native integration with `agy`. Pheobe needs both:
1. A worker adapter (`PHEOBE_PROVIDER=agy` or `antigravity`) so Pheobe can drive `agy` in headless print mode as an execution engine.
2. An adoption kit (`adopt/agy/`) so `agy` agents can easily dispatch Pheobe as an isolated workhorse via progressive-disclosure Skills and Subagents.

## Success criteria

- [x] `PHEOBE_PROVIDER=agy` (and alias `antigravity`) resolves in `worker::REGISTRY`.
- [x] `worker_agy` spawns `agy -p <prompt> --output-format json [flags]` with `cwd` set to the worktree.
- [x] Default flags `--dangerously-skip-permissions` passed so tool execution runs without interactive blocking; `PHEOBE_AGY_FLAGS` replaces defaults when specified.
- [x] Sandbox tier mapping: `strict` maps to `--sandbox`; `moderate` and `free` run standard workspace containment.
- [x] `parse_stdout` extracts `response` as final text, sums tokens from `usage.total_tokens` (or `input_tokens + output_tokens + thinking_tokens`), and extracts `json_tail`.
- [x] Non-zero exit or `status != "SUCCESS"` surfaces clean diagnostics from stderr/stdout.
- [x] `pheobe adopt agy` subcommand prints the AGY adoption kit.
- [x] Adoption kit includes `adopt/agy/skill.md` (Antigravity Skill format for `.agents/skills/pheobe/SKILL.md`) and `adopt/agy/README.md`.
- [x] Full unit test suite in `src/tests/agy.rs` verifying argv, flags, sandbox mapping, timeout, usage parsing, and error reporting.
- [x] `cargo test` passes cleanly with all existing and new tests green (125 tests passed).

## Resolution (2026-09-16)

Implemented `src/worker_agy.rs` and `adopt/agy/` (skill + README):
- `AgyWorker` implements `Worker` trait, spawning `agy -p <prompt> --output-format json [flags]`.
- Defaults to `--dangerously-skip-permissions`, replaceable via `PHEOBE_AGY_FLAGS`.
- `PHEOBE_SANDBOX=strict` maps to `--sandbox`; `moderate`/`free` omit.
- Defensive stdout parse handles preambles (e.g. jetski permission notes), extracts usage tokens, and extracts `json_tail` handoff contract.
- Registered in `src/worker.rs:REGISTRY` under `"agy"` and `"antigravity"`.
- Added `AdoptCmd::Agy` (`pheobe adopt agy`).
- 11 unit tests in `src/tests/agy.rs` covering argv, flags, sandbox, error diagnostics, usage metrics, and report extraction.
- Full suite green: 125 tests passed, clippy and fmt clean.

## Technical approach

- Create `src/worker_agy.rs` modeled after `src/worker_cursor.rs` and `src/worker_claude.rs`.
- Wire `worker_agy::worker` into `src/worker.rs:REGISTRY` for both `"agy"` and `"antigravity"`.
- Add `AdoptCmd::Agy` in `src/main.rs`.
- Create `adopt/agy/README.md` and `adopt/agy/skill.md`.
- Update `adopt/README.md` and `DESIGN.md` documentation tables.
- Add comprehensive test module `src/tests/agy.rs` and wire into `src/tests.rs`.

## Files to modify

- `src/worker_agy.rs` — new worker adapter module
- `src/worker.rs` — register `"agy"` and `"antigravity"` in `REGISTRY`
- `src/main.rs` — add `AdoptCmd::Agy` CLI subcommand and dispatch
- `src/tests/agy.rs` — new unit test module for AGY adapter
- `src/tests.rs` — declare `mod agy` under `tests`
- `adopt/agy/README.md` — AGY adoption overview
- `adopt/agy/skill.md` — AGY Skill definition (`SKILL.md`)
- `adopt/README.md` — harness table row for AGY
- `.jagent/planning/TASKS.md` — track PHEOBE-25 on board

## Non-goals

- In-process Python SDK bindings (CLI subprocess path follows established adapter pattern).
- Interactive TUI mode steering.
