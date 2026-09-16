# Decisions


## 2026-09-16T16:00:07-05:00 — [STRATEGIC] [ADOPTED] [ARCHITECTURAL] Worker adapters spawn each harness's CLI, not its SDK

Reason:
One prompt out, one run back keeps the Worker trait one-shot (PHEOBE-9). Codex and Claude SDKs are CLI wrappers so one kit serves both; Cursor's @cursor/sdk is its own Connect-RPC agent loop, so the cursor kit ships a file form (.cursor/agents/pheobe.md) plus an SDK variant (PHEOBE-23). In-process SDK workers (steering, live USD) are a later trait extension.

Author type: orchestrator

Rejected alternatives:
- **in-process SDK bindings per harness (Node/Python shims)**


## 2026-09-16T16:00:07-05:00 — [STRATEGIC] [ADOPTED] [ARCHITECTURAL] Report mechanical fields are always pheobe's; only contract keys merge from an engine's JSON tail

Reason:
branch, commits (git log base..HEAD since provision) and tests are measured, never narrated. ok/summary/next_steps/doubts/blocked merge from the engine's final JSON — bare, fenced, or embedded (worker::extract_json_tail, PHEOBE-22). Found live: a claude run's commit was missing and its fenced report collapsed to prose before this.

Artifacts: src/run.rs, src/worker.rs

Author type: orchestrator


## 2026-09-16T16:00:07-05:00 — [CONSTITUTIONAL] [ADOPTED] [ARCHITECTURAL] Never cook in the source checkout — worktree or blocked:no_isolation

Reason:
Provision kitchen > buckets > git worktree add; the parent merges, pheobe never pushes a protected branch. Host mode gets the same kitchen via pheobe host setup/finish (PHEOBE-26); a run that dies before the model turn tears its empty worktree down (issue 04).

Author type: orchestrator


## 2026-09-16T16:00:07-05:00 — [STRATEGIC] [ADOPTED] [ARCHITECTURAL] Release channel: version moves with the work; publish the tagged commit

Reason:
release.yml bumps from conventional-commit subjects (feat→minor, fix→patch, !→major, docs/chore no-op), pushes the bump commit BEFORE the tag (zorro#128 ordering), then publish.yml publishes exactly the tagged commit to crates.io (api-drift precedent, Trusted Publishing first). First release is a hand tag v0.1.0. No path/git deps (RULES §5) so the tarball builds alone.

Artifacts: .github/workflows/release.yml, .github/workflows/publish.yml, docs/RELEASING.md

Author type: orchestrator


## 2026-09-16T16:00:07-05:00 — [STRATEGIC] [ADOPTED] [ARCHITECTURAL] LOC budget 500 soft / 1000 hard per source file

Reason:
Fleet doctrine adopted in PHEOBE-21 after tests.rs hit 1444: tests live in src/tests/<area>.rs, a file that would cross 1000 is split, not grown.

Artifacts: .jagent/planning/RULES.md

Author type: orchestrator


## 2026-09-16T16:00:07-05:00 — [STRATEGIC] [ADOPTED] [ARCHITECTURAL] Live coordination is scripts/pheobe-sync; decisions here, status on the board

Reason:
jokersquad agent-sync, one JSONL per primary checkout shared by worktrees (PHEOBE-24) — three agents landed on main in one afternoon with no channel before it. The channel is the conversation; .dejavue/ holds decisions; .jagent/planning/ holds status.

Artifacts: scripts/pheobe-sync, docs/SYNC.md

Author type: orchestrator


## 2026-09-16T16:30:08-05:00 — [STRATEGIC] [VERIFIED] [OPERATIONAL] crates.io publishing is OIDC-only

Reason:
Trusted Publishing configured on the pheobe crate for nixpt/pheobe publish.yml and proven with the verify_auth dispatch (2026-09-16); the CARGO_REGISTRY_TOKEN repo secret was deleted. A future token is only needed for a first publish of a NEW crate.

Author type: orchestrator


## 2026-09-16T17:02:32-05:00 — CLI contract tests are a Cargo [[test]] target under src/tests/cli.rs

Reason:
env!(CARGO_BIN_EXE_pheobe) is only injected for integration tests and benches, not #[cfg(test)] modules, so cli.rs cannot be mod cli in tests.rs. A [[test]] path keeps the file next to the rest of the suite while still spawning the real binary.

Rejected alternatives:
- **unit tests that look up target/debug/pheobe by PATH**: PATH-dependent and misses the cargo-provided bin env
- **tests/cli.rs at the crate root**: works, but the ticket pinned src/tests/cli.rs


## 2026-09-16T17:29:53-05:00 — Shim writers use tmp+chmod+rename, not in-place write

Reason:
ETXTBSY happens when execve hits an inode still open for write. Writing a sibling tmp and renaming onto the dest means the dest inode is never a write-fd, so a parallel test's fork cannot busy the file being exec'd. Retry on errno 26 is belt-and-suspenders at spawn.

Rejected alternatives:
- **spawn through sh <path>**: changes argv[0] and would hide missing +x
- **serialize all adapter tests**: slower than the race is rare

