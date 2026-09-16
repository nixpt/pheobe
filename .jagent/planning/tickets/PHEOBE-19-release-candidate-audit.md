# PHEOBE-19 — release candidate audit: license, README, CI gate, lint

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-19 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W5 — release |
| **Assignee** | foreman (s457) |
| **Dependencies** | PHEOBE-18 |
| **Estimated effort** | S |

## Problem

PHEOBE-18 proved `cargo publish --dry-run` passes, but a foreman review
(s457, 2026-09-16) found the tree was not yet something to hand to a
stranger: `license = "MIT OR Apache-2.0"` with only `LICENSE-MIT` on disk;
no `README.md` (the crate's `readme` pointed at the 1,000-line DESIGN.md);
`cargo clippy -D warnings` failing on 13 lints and `cargo fmt --check`
dirty; and **no CI test job at all** — `.github/workflows/` held only
`release.yml`, which bumps + tags + creates a GitHub release on every push
to `main`, ungated. On the first push to a fresh `nixpt/pheobe` that would
have tagged v0.1.1 off an unlinted tree.

## Success criteria

- [x] `LICENSE-APACHE` present alongside `LICENSE-MIT`; README states the dual license
- [x] `README.md` — what it is, install, quick start (task + report shapes), commands,
      env configuration, adoption kits, pointer to DESIGN.md; every claim checked
      against `src/` (no config file exists; sandbox default is `moderate`; env wins)
- [x] `Cargo.toml` `readme = "README.md"`
- [x] `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --check` clean
- [x] `.github/workflows/ci.yml`: fmt + clippy + test + `cargo package --no-verify`
      on every push and PR
- [x] `release.yml` runs the same gate in-line before `bump-version.sh`, so a red
      tree is never tagged
- [x] `.jagent/planning/TASKS.md` brought in line with ticket `Status` fields
      (it still listed PHEOBE-2 as unmerged and issues 01–03 as open)
- [x] ticket template said `EXS-NNN` (scaffold artifact) — now `PHEOBE-NNN`

## Non-goals

- Creating the GitHub remote or publishing to crates.io — captain's call
  (PHEOBE-18 "held off"). When promoting: create `nixpt/pheobe`, push
  `main`, and the first push will run ci.yml and release.yml together.
- Scrubbing box-local provenance paths (`/workspace/external/…`) from the
  `adopt/` and `knowledge/` docs that ship in the crate. They are source
  citations, not secrets; flagged for the captain, not changed here.
- Issue 04 (failed model run leaves an empty worktree) — open, P3, separate.

## Resolution

Branch `agent/foreman/PHEOBE-19-rc-audit` from `fec675f`. Verified:
100 tests green, clippy/fmt clean, `cargo package` builds the tarball and
the isolated verify build passes.
