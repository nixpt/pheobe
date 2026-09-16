# PHEOBE-22 — worker-route report fidelity: issues 05–09 + kit drift

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-22 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W6 |
| **Assignee** | foreman (s457) |
| **Dependencies** | PHEOBE-9, PHEOBE-21 |
| **Estimated effort** | S |

## Problem

The s457 adoption test drove all three routes (self via flownet, host with
`pheobe verify`, `PHEOBE_PROVIDER=claude`) to `ok:true`, but the worker
route's report was not the contract: the engine's commit was missing
(issue 05), a ```json-fenced report collapsed into prose (06), the prompt
told an external engine to call `plan_tracker`/`verify`/`handoff` tools it
does not have (07), `pheobe verify` gave the host no reason on a
`done_when` failure (08), and — found while re-testing — a tree dirty only
from `__pycache__/` killed the run with an empty error (09). Plus kit
drift: `pheobe run … --json` in five kit files with no such flag, DESIGN.md
naming kit files that do not exist, `testparse` calling a stdlib `ok <name>`
runner "go".

## Success criteria

- [x] `commits` = `git log <base>..HEAD` since provision (base sha recorded in `run_task`);
      the gate's own commit is one more entry, not the only source
- [x] one `worker::extract_json_tail` (bare → last ```json fence → widest `{…}`, handoff-shaped
      only, empty `blocked` dropped) used by claude/cursor/codex/opencode; opencode's private
      copy removed
- [x] `PromptMode::{SelfLoop, Worker}`: same persona/contract/task body; the worker variant
      says plan file + done_when command + commit-with-trailer + "END by printing the report,
      there is no handoff tool here"
- [x] `pheobe verify` prints `✗ done_when failed: <cmd>` + the output excerpt before bailing
- [x] `worktree::commit` → `Option<String>`, no-op when nothing is staged; `run()` reports
      git's stdout when stderr is empty
- [x] `pheobe run --json` accepted (no-op — JSON is the only output); DESIGN.md kit table
      lists the real files and names the missing claude host kit; `testparse` labels bare
      ok/FAIL lines `"ok-lines"`
- [x] tests: `src/tests/worker_route.rs` (7, incl. Claude's verbatim tail from the run) —
      109 green, clippy/fmt clean; all touched files under the 500 soft line

## Resolution

Live re-run of the claude worker route on the fixed binary: `commits: ["a15d003"]`,
`Pheobe-Task` trailer present, summary/next_steps/doubts from the fenced JSON, runner
`ok-lines`, `__pycache__/` left behind without failing the run. `pheobe verify` on a
failing tree prints the command and the two failing test lines.

## Non-goals

A native claude host kit (listed as a gap in DESIGN.md's table); the
`/workspace/external/…` provenance citations (PHEOBE-20 flag, still the
captain's call).
