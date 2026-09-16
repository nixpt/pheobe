# Contributing to pheobe

pheobe is small on purpose: one binary, no daemon, no path deps, a JSON
contract that adopting harnesses can rely on. Contributions that keep it that
way are welcome — bug reports with a reproducing task file most of all.

## Ground rules

- **Work on a branch in a worktree**, never on `main`. pheobe practises what it
  preaches: `git worktree add ../pheobe-<topic> -b <topic>`.
- **Conventional commits.** `feat:` mints a minor version, `fix:`/`perf:` a
  patch, `feat!:` or a `BREAKING CHANGE:` footer a major; `docs:`, `chore:`,
  `test:`, `ci:`, `refactor:` mint nothing. Releases are automatic from these
  subjects (`docs/RELEASING.md`), so the prefix is not decoration.
- **One change per PR**, with the ticket or issue it closes in the body.
- **No path or git dependencies.** The crate must build from its own tarball;
  CI runs `cargo package` to prove it.
- **File-size budget:** 500 lines soft, 1000 hard per source file. Split by
  area (`src/tests/<area>.rs` is the pattern) rather than grow.

## Before you push

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo package --no-verify
```

That is exactly what `ci.yml` runs. Adapter tests exec small shell shims; under
heavy parallel load one may hit `ETXTBSY` (issue 10) — re-run before assuming
a real failure.

## The contract is the product

`README.md` → "Quick start" shows the task file and the handoff report. If a
change alters either shape, it is a breaking change even when every test
passes: bump accordingly and update `adopt/README.md`, which every adoption kit
points at. Mechanical report fields (`branch`, `commits`, `tests`) are computed
by pheobe from git and the `done_when` run — never sourced from model output.

## Adding a worker adapter

An adapter spawns a harness's CLI and returns its final message; pheobe owns
everything around it. Copy the shape of `src/worker_cursor.rs`:

1. `impl Worker for XWorker` in `src/worker_x.rs`; register the name(s) in
   `worker::REGISTRY`.
2. Environment contract: `PHEOBE_X_BIN`, `PHEOBE_X_FLAGS` (replaces the
   default entirely), `PHEOBE_X_TIMEOUT_SECS`. Document the exact argv you
   verified against the real binary's `--help` in the module doc.
3. Map `PHEOBE_SANDBOX` (`strict | moderate | free`) onto whatever the harness
   offers and say what collapses.
4. Parse stdout defensively; hand the final text to
   `worker::extract_json_tail` — do not write another JSON scraper.
5. Tests in `src/tests/x.rs` using shell shims (argv, flags, sandbox mapping,
   timeout, usage parsing, error surfacing). No test may need the real harness
   or network.
6. An adoption kit in `adopt/x/` (host protocol and/or self-mode dispatcher
   def) and a row in `adopt/README.md`; wire `pheobe adopt x`.

## Adding a knowledge entry

`knowledge/*.md` is the seed research drive: front matter (`slug`, `name`,
`kind`, `version`, `last_verified`, `verified_by`, `cutoff_gap`, `tags`,
`sources`) plus a short body of verified facts and gotchas. Cite public sources
(a docs URL, a repository, a crates.io/npm/PyPI page, a version you read),
never a path on your machine. Keep an entry under ~90 lines: it is injected
into prompts.

## Reporting a defect

Open a GitHub issue with the task JSON, the `PHEOBE_*` environment you used,
the handoff report (or the stderr when there was none), and the harness
version if a worker adapter was involved. In-repo, field defects also live as
`.jagent/issues/NN-*.md`.

## License

By contributing you agree your work is licensed under the project's dual
MIT OR Apache-2.0 terms.
