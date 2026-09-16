# TASKS — pheobe

Planned work lives in `tickets/PHEOBE-N.md`; observed defects live in `../issues/NN-*.md`
and are listed here so the board is the one place to look.

---

## P0 — Build & Core Health

- [x] `cargo build --release` clean (verified 2026-09-16 at `c828bec`)
- [ ] `cargo test` green (no test suite yet beyond `src/tests.rs`)
- [ ] **issue 01** (`../issues/01-allowlist-off-by-one-first-porcelain-line.md`) — `check_allowlist` mangles the first `git status --porcelain` line (`M calc.py` → `alc.py`) because `worktree::run()` trims stdout; every run with a modified file is blocked at the gate.
- [ ] **issue 02** (`../issues/02-own-state-dir-counted-as-allowlist-violation.md`) — `.pheobe/` and `done_when` byproducts (`__pycache__/`) are reported as allowlist violations.
- [ ] **issue 03** (`../issues/03-existing-slug-branch-reused-silently.md`) — a pre-existing `pheobe/<slug>` branch is checked out silently instead of refusing or suffixing.

## M0 — the loop exists

- [x] PHEOBE-1 — the model turn (`c828bec`)
- [ ] PHEOBE-2 — aging ladder + budget enforcement (`agent/nixp/PHEOBE-2`, unmerged)

## M1 — adoptable

- [x] PHEOBE-9 — the `Worker` trait + `PHEOBE_PROVIDER` dispatcher (`agent/nixp/PHEOBE-9-worker-trait`)
- [ ] adoption kits verified against a real harness each
- [ ] host mode exit gate (`pheobe verify`)
