# PHEOBE-18 — release posture: crates.io publish + workspace wiring

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-18 |
| **Priority** | P3 |
| **Status** | Done |
| **Assignee** | nixp |
| **Dependencies** | PHEOBE-13 (dogfood green) + PHEOBE-9 (worker trait) |
| **Estimated effort** | S |

## Problem

pheobe's posture (DESIGN.md): self-contained, `cargo install`-able
anywhere, no path-deps in the default build. Once the worker quartet
lands, the repo needs the standard fleet release kit so adopt kits can
say `cargo install pheobe` and parent repos can wire it as a workspace
dep (e.g. mayfly's future `harness: "pheobe"`).

## Success criteria

- [x] `cargo publish` readiness: package metadata complete (license
      dual MIT OR Apache-2.0, description, keywords/categories already
      in Cargo.toml), `cargo doc --no-deps` zero warnings, default
      build has no peer path-deps (`cargo tree` audit).
- [x] GitHub release workflow (bump-version.sh + release.yml, mayfly's
      v0.1.x precedent), remote `nixpt/pheobe` created.
- [x] `pheobe/DESIGN.md` position-table updated with the real remote +
      version.
- [x] AGENTS.md (workspace) gains the pheobe entry with lineage one-liner
      (currently only the task-ID row exists).

## Resolution

- **Publish readiness, proven live:** `cargo publish --dry-run
  --allow-dirty` passes end-to-end (packaged 55 files / 477.4KiB,
  isolated verify build green, upload aborted only because dry-run).
  `cargo tree` has zero path/git deps; `cargo doc --no-deps` emits zero
  warnings. Cargo.toml gains `repository = "https://github.com/nixpt/pheobe"`,
  version bumped 0.0.1 → **0.1.0** (mayfly precedent: initial release
  hand-picked, then the bump kit owns every later bump), and
  `exclude = [".jagent", ".github", "scripts", ".gitignore"]` so the
  tarball ships src + persona/ + adopt/ (both include_str!-required) +
  knowledge/ (seed corpus for ~/.pheobe/knowledge) but no planning/CI
  artifacts. LICENSE-MIT added (mayfly's, re-titled to pheobe
  contributors), matching the declared `MIT OR Apache-2.0`.
- **Release kit installed** via `squadron/bin/bump-version --install`
  (mayfly v0.1.x precedent): `scripts/bump-version.sh` +
  `.github/workflows/release.yml`. Two install-time corrections: the
  installer baked the current *task branch* as the default branch
  (pheobe had no remote to detect from) — release.yml trigger +
  BUMPVER_MAIN_BRANCH pinned to `main`; release-bot identity aligned to
  `pheobe-release` (guard + GIT_USER + script default), matching
  mayfly's `mayfly-release`. Dry-run confirms correct skip behavior on
  non-main branches.
- **DESIGN.md:** stale "no code yet" status line replaced with the real
  posture — v0.1.0, cargo-installable, remote, 100 green tests, live ACP
  dispatch proven.
- **Workspace AGENTS.md:** pheobe peer entry added under the mayfly
  sibling block (lineage one-liner: headless coding workhorse, worker
  adapters, ACP surface, memory trait, no path-deps, remote @ v0.1.0).
