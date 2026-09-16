# TASKS — pheobe

Planned work lives in `tickets/PHEOBE-N-*.md`; observed defects live in `../issues/NN-*.md`
and are listed here so the board is the one place to look. Ticket `Status` fields are the
source of truth; this board mirrors them (rebuilt 2026-09-16, s457).

---

## P0 — Build & Core Health

- [x] `cargo build --release` clean
- [x] `cargo test` green — 117 tests (`src/tests.rs` + per-module tests)
- [x] `cargo clippy --all-targets -- -D warnings` + `cargo fmt --check` clean (PHEOBE-20)
- [x] CI: `.github/workflows/ci.yml` (fmt, clippy, test, package) + release gate (PHEOBE-20)
- [x] **issue 01** — allowlist off-by-one on the first porcelain line (Done, PHEOBE-3)
- [x] **issue 02** — own state dir counted as an allowlist violation (Done, PHEOBE-3)
- [x] **issue 03** — existing slug branch reused silently (Done, PHEOBE-3)
- [x] **issue 04** (`../issues/04-failed-model-run-leaves-empty-worktree.md`) — a run that
      dies before the model turn (endpoint/auth failure) leaves an empty worktree + branch
      behind. P3 hygiene. **Done, PHEOBE-26.**
- [x] **issue 05** (`../issues/05-worker-route-drops-commits-the-engine-made.md`) — worker route
      reports no `commits` when the engine committed itself (found: claude adapter, s457). P2.
- [x] **issue 06** (`../issues/06-json-tail-misses-fenced-report.md`) — a ```json-fenced handoff
      report is not parsed; `next_steps`/`doubts` collapse into prose `summary`. P2.
- [x] **issue 07** (`../issues/07-worker-prompt-advertises-self-mode-tools.md`) — worker prompt
      tells external engines to call `plan_tracker`/`verify`/`handoff` tools they don't have. P2.
- [x] **issue 08** (`../issues/08-verify-prints-no-reason-on-done-when-failure.md`) — `pheobe verify`
      gives the host no reason on a done_when failure. P3.
- [x] **issue 09** (`../issues/09-byproduct-only-dirty-tree-fails-the-commit-gate.md`) — a tree dirty
      only from byproducts killed the run with an empty error (Done, PHEOBE-22).
- [ ] **issue 10** (`../issues/10-shim-exec-etxtbsy-race-in-parallel-tests.md`) — adapter tests can
      hit ETXTBSY when a parallel test forks mid-shim-write (codex, sandboxed run). P4 flake.
- [x] kit drift found s457 (Done, PHEOBE-22): `adopt/claude/self-agent.md` runs `pheobe run <file> --json` (no such
      flag); DESIGN.md's kit table lists `adopt/claude/host/agents/pheobe.md` which does not exist
      (claude has no host kit — the opencode host protocol was used instead); `testparse` labels a
      Python stdlib `ok <name>` runner as `"runner": "go"` (cosmetic).

## M0 — the loop exists

- [x] PHEOBE-1 — the model turn
- [x] PHEOBE-2 — aging ladder + budget enforcement
- [x] PHEOBE-3 — field defects 01–03
- [x] PHEOBE-13 — live dogfood on fixed main (deepseek-v4-pro / kimi-k2.6 / glm-5.2, all `ok:true`)

## M1 — adoptable

- [x] PHEOBE-9 — the `Worker` trait + `PHEOBE_PROVIDER` dispatcher
- [x] PHEOBE-10 — barn hardening: structured test parsing, format-on-write, checkpoints
- [x] PHEOBE-12 — knowledge drive seeding (language passports + fleet ctx entries)
- [x] PHEOBE-15 — adopt-kit expansion + alignment
- [x] PHEOBE-4 / 5 / 7 — opencode, claude, codex worker adapters (each live-smoked)
- [x] PHEOBE-6 / 8 — cursor, kimi worker adapters
- [x] host mode exit gate (`pheobe verify`; `pheobe host setup`/`finish` as of PHEOBE-26)

## M2 — isolation, structure, memory, transport

- [x] PHEOBE-14 — sandboxing ladder (bwrap: strict / moderate / free)
- [x] PHEOBE-11 — structural read/write ladder (polydex + code-atlas)
- [x] PHEOBE-16 — `PHEOBE_MEMORY=none|local|host` trait swap
- [x] PHEOBE-17 — `pheobe acp --stdio` (bro synapse dispatch peer)

## W6 — fidelity

- [x] PHEOBE-21 — LOC budget: split tests.rs / tools.rs, adopt the 500/1000 rule
- [x] PHEOBE-22 — worker-route report fidelity: issues 05–09 + kit drift (s457 adoption test)
- [x] PHEOBE-23 — Cursor SDK/CLI alignment: --sandbox mapping, usage/cost parse, file-form kit (cursor)
- [x] PHEOBE-24 — agent-sync channel: `scripts/pheobe-sync` + `docs/SYNC.md` + RULES §7
- [x] **PHEOBE-23** — Cursor SDK/CLI alignment (`--sandbox` map, IDE agents kit, `disallowedTools` on AgentOptions)
- [x] **PHEOBE-25** (`tickets/PHEOBE-25-agy-worker-and-adopt-kit.md`) — AGY worker adapter (`PHEOBE_PROVIDER=agy`) + adoption kit (`adopt/agy/`)
- [x] PHEOBE-26 — host supervisor (`pheobe host setup`/`finish`) + issue 04 empty-worktree teardown

## W5 — release

- [x] PHEOBE-18 — release posture: publish readiness, bump kit, `release.yml`
- [x] PHEOBE-20 — release candidate audit: LICENSE-APACHE, README, CI gate, clippy/fmt
- [x] PHEOBE-27 — release channel checked + `publish.yml` (crates.io) added; `docs/RELEASING.md`
- [ ] promote: `gh repo create nixpt/pheobe` + push `main` + hand-tag `v0.1.0` → publish (captain's call, PHEOBE-27)
- [ ] decide whether the `/workspace/external/…` provenance citations in `adopt/` and
      `knowledge/` should ship in the crate as-is (PHEOBE-20 non-goal, flagged)
