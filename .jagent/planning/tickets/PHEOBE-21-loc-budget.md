# PHEOBE-21 — LOC budget: split the hard-block file, adopt the fleet rule

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-21 |
| **Priority** | P3 |
| **Status** | Done |
| **Assignee** | nixp |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

pheobe has no LOC budget and the growth curve is steep (7.7k lines after W5,
three capability arcs in five weeks). Against the fleet rule (emoe-workspace
`.dejavue/rules.md`, itself the "1000-line file lesson" turned standing
rule): `src/tests.rs` is at **1444 — hard-block violation** (no source file
may exceed 1000 LOC; the wrong module boundary), and `src/tools.rs` is at
**524 — over the 500 soft block**. `memory.rs` (483) and `structint.rs`
(450) approach the soft line. Every week of growth makes the split harder.

## Success criteria

- [x] `src/tests.rs` split into focused module files under `src/tests/`,
      none over the 500 soft line.
- [x] `src/tools.rs` split into focused modules under `src/tools/`, none
      over the 500 soft line.
- [x] No behavior change: 102 tests green, zero warnings, `cargo publish
      --dry-run` still passes.
- [x] RULES.md gains the standing LOC budget (500 soft / 1000 hard, the
      emoe-workspace doctrine) so future arcs hit a named rule, not a
      surprise.

## Resolution

- **tests** (1444 → root 61 + 10 files, largest claude.rs 275): contract,
  scripted (shared ScriptedProvider), agent_loop, aging, gates, outcome,
  p10, opencode, claude, e2e. Shared harness (run_git, git_repo_with,
  worker_task, env knobs, claude_env_lock) lives in the root.
- **tools** (524 → root 95 + 5 files, largest exec 110): fs (read/write/
  edit), search (glob/grep), exec (bash + safe-exec guard), meta (plan/
  verify/ctx/handoff), structural (polydex ladder). Root keeps ToolCtx/
  Tool/barn/schemas/dispatch + the path gate (resolve/check_allow).
- Dead code removed by the split: `run_bash` (the pre-PHEOBE-14 moderate
  convenience wrapper — no callers since everything went tiered).
- Largest source file now `memory.rs` 483 — under the soft line.
- Verified in the worktree: 102 green, zero warnings, publish dry-run
  clean.

## Technical approach

- Moderate modules within the one crate (not crates/): pheobe is a single
  self-contained crate by posture (PHEOBE-18); the emoe "split into a
  crate" clause applies to workspaces, the module equivalent applies here.
- tests: `src/tests.rs` becomes the module root (shared helpers) + one file
  per section: contract, agent_loop, aging, gates, worker_outcome, p10,
  opencode, claude, run_e2e.
- tools: root keeps ToolCtx/Tool/barn/schemas/dispatch + path gate helpers;
  children: fs (read/write/edit), search (glob/grep), exec (bash +
  safe-exec guard), meta (plan/verify/ctx/handoff), structural (polydex
  reads).
