# PHEOBE-32 — `pheobe update` installs from crates.io (git fallback)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-32 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W5 — release |
| **Assignee** | nixpt/cursor |
| **Dependencies** | PHEOBE-31 (channel probe), crates.io `pheobe` 0.2.0 |
| **Estimated effort** | S |

## Problem

crates.io `pheobe` 0.2.0 is live, but the binary has no upgrade command.
README still says `cargo install --path .`. Bro's GitHub-asset updater is
the wrong shape (private repo, octostream, target triples). Copy
secure-env's spawn-cargo surface, pointed at the public crate.

## Success criteria

- [x] `pheobe update --check` prints the same version line as doctor;
      exit 1 iff an update is available.
- [x] `pheobe update` runs `cargo install pheobe --locked --force` when
      crates.io has the crate; otherwise
      `cargo install --git https://github.com/nixpt/pheobe --tag vX.Y.Z
      --locked --force`. No-op when already current unless `--force`.
- [x] Channel unreachable → error, no cargo. `cargo` missing → clear error.
- [x] Tests cover argv selection + should-install with a fake probe.
- [x] README install is `cargo install pheobe`.

## Resolution

`pheobe update` reuses the PHEOBE-31 probe and spawns `cargo install`.
Live `--check` against crates.io 0.2.0: `pheobe: 0.2.0 (current)`, exit 0.
15 update-module tests; clippy `-D warnings`; `fmt --check`.

## Technical approach

- Reuse `update::check` / `Status` from PHEOBE-31.
- `cargo_install_args` + `should_install` are pure; `run` spawns cargo
  with inherited stdio.
- Not bro's in-place binary replace.

## Files to modify

- `src/update.rs`, `src/main.rs`, `src/tests/update.rs`
- `README.md`, `DESIGN.md`, `TASKS.md`, this ticket

## Non-goals

- GitHub Release prebuilt assets / `cargo-binstall`
- stable/nightly channels
- Amending `bump-version` / `release.yml`
