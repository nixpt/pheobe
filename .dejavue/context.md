---
name: pheobe
purpose: A coding workhorse with no face — task JSON in, worktree branch + handoff report out. Built to be adopted as a subagent by any harness.
dcp: DCP/1.0
---

# Context

<!-- The DCP instruction layer: what an agent should *do* in this repo.
     Source of truth — adapters (CLAUDE.md / AGENTS.md / …) are generated
     from this file via `dejavue export --target <tool>`. -->

## Operating Rules

- Read `.jagent/planning/RULES.md` first — one worktree per ticket, never commit to `main`, verify a ticket's status before working it, file new defects as their own issue, LOC budget 500 soft / 1000 hard.
- Coordinate live through `scripts/pheobe-sync` (`read --tail 30` on arrival, `claim` before overlapping edits, `done` at handoff). Several agents work this repo at once.
- Planned work is a `PHEOBE-N` ticket in `.jagent/planning/tickets/`; observed defects are `.jagent/issues/NN-*.md`; the board is `.jagent/planning/TASKS.md`.
- The handoff report is the contract. Mechanical fields (`branch`, `commits`, `tests`) are always computed by pheobe, never taken from a model's prose.

## Build / Test

- `cargo test` (all tests; adapter tests exec shell shims — issue 10 ETXTBSY flake under parallel load), `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check`, `cargo package --no-verify`. CI (`ci.yml`) runs exactly those; `release.yml` runs them again before any version bump.
- No path or git deps — the crate must build from its own tarball (`cargo publish --dry-run`).

## Architecture Map

- `src/run.rs` — `run_task`: intake → worktree → orient (knowledge brief, structural brief, learned nudges) → engine → mechanical gates (allowlist, commit, done_when) → report. The one surface `pheobe run` and `pheobe acp` share.
- `src/agent.rs` — the self-mode loop (tool-calls: plan_tracker / verify / handoff) and `run_worker` (one prompt out to an external engine, one run back). `PromptMode::{SelfLoop, Worker}` render the two rule sets.
- `src/worker*.rs` — `Worker` trait + adapters: opencode, claude, codex, cursor, kimi, agy. Each spawns that harness's CLI; `worker::extract_json_tail` normalises the engine's final message.
- `src/worktree.rs` — provision (kitchen > buckets > git worktree), allowlist check, pathspec commit with `Pheobe-Task:` trailer, `commits_since` base.
- `src/verify.rs` + `src/testparse.rs` — `done_when` execution and runner-aware pass/fail parsing.
- `src/host.rs` — host-mode supervisor (`pheobe host setup|finish`) so an adopting harness's model gets pheobe's kitchen and exit gate.
- `src/acp.rs` — Agent Client Protocol server over stdio (`bro synapse dispatch -- pheobe acp --stdio`).
- `src/knowledge.rs` — the research drive (`knowledge/` seeds `~/.pheobe/knowledge`); `src/learn.rs` + `src/memory.rs` — closed-loop lessons, `PHEOBE_MEMORY=none|local|host`.
- `adopt/<harness>/` — one adoption kit per harness; `pheobe adopt <name>` prints it. `persona/pheobe.md` is the system prompt's identity layer.

## Memory

Decisions, blockers, and constraints are captured in `.dejavue/` — run
`dejavue context` for the boot packet and `dejavue recall <query>` to search.
