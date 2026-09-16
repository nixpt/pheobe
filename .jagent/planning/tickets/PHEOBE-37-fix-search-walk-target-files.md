# PHEOBE-37 — fix `search::walk` skipping files starting with "target" (issue 11)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-37 |
| **Priority** | P3 |
| **Status** | Done |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | nixpt/agy |
| **Dependencies** | PHEOBE-35 |
| **Estimated effort** | S |

## Problem

In `src/tools/search.rs:44`:

```rust
let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
if name == ".git" || name.starts_with("target") || name == "node_modules" {
    continue;
}
```

The check `name.starts_with("target")` was intended to skip Rust build output directories (`target/`, `target-*`), but was applied to all entries in `walk()`, skipping legitimate source files like `target.rs`, `targeting.py`, or `target_triple.c`.
This was discovered in PHEOBE-35 and recorded as field defect Issue 11 (`.jagent/issues/11-search-walk-skips-any-file-named-target.md`).

## Success criteria

- [x] `walk()` in `src/tools/search.rs` only skips `target*` entries if they are directories (`is_dir && name.starts_with("target")`)
- [x] Regular files starting with `"target"` (e.g. `target.rs`, `targeting.py`) are properly indexed and returned by `glob` and `grep`
- [x] Directories starting with `"target"` (e.g. `target/`, `target-debug/`) continue to be skipped
- [x] Tests updated: `search_walk_skips_files_starting_with_target_pinned_issue_11` updated/renamed to assert the fixed behavior (files found, directories skipped)
- [x] All tests passing, `cargo fmt`, `cargo clippy`, and `cargo package --no-verify` clean

## Resolution

- In `src/tools/search.rs:walk`, extracted `let is_dir = p.is_dir();` and changed the ignore filter condition to:
  `if name == ".git" || name == "node_modules" || (is_dir && name.starts_with("target")) { continue; }`
- Updated test in `src/tests/search.rs` (`search_walk_includes_files_starting_with_target_issue_11`) verifying that `target.rs` and `targeting.py` are returned in `grep` and `glob` results while entries within directories starting with `target` (`target-build/sub.rs`) are skipped.
- Updated `.jagent/issues/11-search-walk-skips-any-file-named-target.md` with resolution notes.

## Technical approach

1. Check `let is_dir = p.is_dir();` before the ignore check in `src/tools/search.rs:walk`.
2. Update filter condition to `if name == ".git" || name == "node_modules" || (is_dir && name.starts_with("target"))`.
3. Update `src/tests/search.rs` test for Issue 11 to verify that `target.rs` and `targeting.py` are found, while a directory named `target-dir/` or `target/` is skipped.
4. Update Issue 11 resolution note.
5. Verify test suite, clippy, fmt, and package.

## Files to modify

- `src/tools/search.rs` — gate `name.starts_with("target")` on `is_dir`
- `src/tests/search.rs` — update test to verify fixed behavior
- `.jagent/issues/11-search-walk-skips-any-file-named-target.md` — mark resolved with PHEOBE-37 reference

## Non-goals

- Altering any other search semantics or ignore rules.
