# PHEOBE-18 — release posture: crates.io publish + workspace wiring

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-18 |
| **Priority** | P3 |
| **Status** | Backlog |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-13 (dogfood green) + PHEOBE-9 (worker trait) |
| **Estimated effort** | S |

## Problem

pheobe's posture (DESIGN.md): self-contained, `cargo install`-able
anywhere, no path-deps in the default build. Once the worker quartet
lands, the repo needs the standard fleet release kit so adopt kits can
say `cargo install pheobe` and parent repos can wire it as a workspace
dep (e.g. mayfly's future `harness: "pheobe"`).

## Success criteria

- [ ] `cargo publish` readiness: package metadata complete (license
      dual MIT OR Apache-2.0, description, keywords/categories already
      in Cargo.toml), `cargo doc --no-deps` zero warnings, default
      build has no peer path-deps (`cargo tree` audit).
- [ ] GitHub release workflow (bump-version.sh + release.yml, mayfly's
      v0.1.x precedent), remote `nixpt/pheobe` created.
- [ ] `pheobe/DESIGN.md` position-table updated with the real remote +
      version.
- [ ] AGENTS.md (workspace) gains the pheobe entry with lineage one-liner
      (currently only the task-ID row exists).
