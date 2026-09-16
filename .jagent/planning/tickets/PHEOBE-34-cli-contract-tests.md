# PHEOBE-34 — CLI contract tests: drive the real binary (main.rs is 0% covered)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-34 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | unassigned — claim on `scripts/pheobe-sync` |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`cargo llvm-cov` (2026-09-16, 148 tests): `src/main.rs` is **0% covered** —
220 of the 1,105 uncovered lines in the crate. Every subcommand handler
(`run`, `verify`, `host`, `adopt`, `ctx`, `learn`, `check`, `doctor`, `acp`,
`update`) is exercised only by hand. The CLI *is* the contract adopting
harnesses depend on (README "Commands" table, every `adopt/` kit), and
nothing proves it.

## Success criteria

- [ ] `src/tests/cli.rs` spawns the built binary via
      `env!("CARGO_BIN_EXE_pheobe")` (no PATH dependence), with `HOME` and
      `PHEOBE_KNOWLEDGE_DIR` pointed at a scratch dir so no test touches the
      user's `~/.pheobe`
- [ ] covered: `verify` pass + fail (exit 0/1, the `✗ done_when failed:` reason
      on stderr — issue 08 contract); `host setup` → `host finish` round trip on
      a fixture repo; `adopt <every kit>` prints the kit and unknown names fail;
      `ctx seed` (writes N, then keeps), `ctx list`, `ctx brief --for-repo` on a
      Cargo.toml fixture contains the Rust body; `doctor` with a dead proxy
      (`HTTPS_PROXY=http://127.0.0.1:9`) prints `channel unreachable` and exits 0;
      `run` on a task without `done_when` is refused at intake with a clear
      message and no worktree created; `run --json` accepted; `update --check`
      offline exits non-zero with a reason
- [ ] no test needs a model, a network, or a harness binary
- [ ] `main.rs` line coverage ≥ 70%; total ≥ 82% (was 79.31%)
- [ ] `cargo test` stays under ~15 s; the CLI tests share one fixture builder

## Technical approach

- One helper `fn pheobe(args, env) -> Output` in `src/tests/cli.rs`; fixture
  repo via the existing `git_repo_with` in `src/tests.rs`.
- Assert on exit code + the documented stdout/stderr lines, not on incidental
  wording — these tests define the CLI contract from now on.
- Keep it under the 500-line soft budget; split by subcommand if it grows.

## Non-goals

`acp` over a real ACP client (covered by `src/tests/acp.rs`); the live
network half of `update`/`doctor` (by design untested).
