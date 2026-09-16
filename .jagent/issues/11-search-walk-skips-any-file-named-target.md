# issue 11 — `search::walk` skips files whose names begin with "target"

Found during PHEOBE-35 search tool tests (2026-09-16, s457).

## Observation

In `src/tools/search.rs:44`:

```rust
let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
if name == ".git" || name.starts_with("target") || name == "node_modules" {
    continue;
}
```

The check `name.starts_with("target")` was intended to skip Rust build output
directories (e.g. `target/`, `target-something/`), but it applies to every entry
in the walk, including regular files.

Any legitimate source file whose name starts with `"target"` (e.g. `target.rs`,
`targeting.py`, `target_triple.c`) is silently omitted from both `glob` and
`grep` searches.

## Behavior pinned in PHEOBE-35

Per RULES §1 ("verify before you fix" + "do not fix and test in one commit"),
PHEOBE-35 pins this current behavior in tests without changing the walk implementation.

## Fix

When addressing this issue: only skip when `p.is_dir() && name.starts_with("target")`,
or check `name == "target" || (p.is_dir() && name.starts_with("target"))`.

## Resolution

Resolved in PHEOBE-37:
- Changed `walk()` filter to `if name == ".git" || name == "node_modules" || (is_dir && name.starts_with("target"))`.
- Updated test `search_walk_includes_files_starting_with_target_issue_11` in `src/tests/search.rs` to verify that regular files starting with `target` (`target.rs`, `targeting.py`) are searched while directories starting with `target` (`target-build/`) continue to be skipped.

