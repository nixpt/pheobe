# PHEOBE-51 — worktrees inside fleet repos (`.jagent/worktrees`, squadron SQ-204)

**Filed:** 2026-09-25, foreman, at the captain's direction.

## Problem
`provision` placed every worktree as a sibling of the repo, either through
buckets or as a plain `../<repo>-<branch>`. An engine pointed at the repo
therefore worked outside it:
- sandboxes needed extra grants (`--add-dir`, `free` tier);
- the ANTA-44 derby's horses landed in `/workspace/worktrees/`;
- a free-tier horse planted peer symlinks outside its sandbox to make sibling
  path-deps resolve.

The doc comment also claimed a "kitchen >" rung that was never implemented.

## Fix
A repo with `.jagent/` (the fleet layout) gets its worktree at
`<repo>/.jagent/worktrees/<branch with / → ->`. Sibling path-deps keep
resolving through `.jagent/worktrees/<sib> -> ../../../<sib>` links.

`src/siblings.rs` is a dependency-free port of squadron's
`worktree_scan_siblings` / `worktree_link_siblings`, the same rule as buckets
BUCKETS-17 (buckets#6). It covers tracked `Cargo.toml` `path = "../…"` deps,
including nested manifests, and tracked `../` symlinks. Only one-level climbs
out of the repo count.

buckets receives `--path`. A buckets older than BUCKETS-17 rejects the flag,
and pheobe then falls back to plain `git worktree add` at the same in-repo
path. Other repos keep the sibling layout.

Also:
- `.jagent/worktrees/` is now gitignored, where it had only been a local
  exclude;
- the ladder doc comment and the architecture map in the context file are
  corrected.

## Verified
- 6 tests:
  - `siblings`: `path_deps` parsing, one-level normalization, `in_repo_dest`;
  - `tests/placement.rs`: a fleet repo's worktree is inside the repo and its
    nested `../../../peer` resolves, a plain repo keeps the sibling layout, and
    an old buckets without `--path` falls back to git in-repo.
- Gates: fmt, clippy `-D warnings`, full suite (223), `cargo package`.
- One full-suite run hit a pre-existing timing flake in
  `parity::engine_error_before_any_commit_stays_never_ran` (62 s run under
  load). It passes alone 3/3, and in two more full runs at 15 s.
