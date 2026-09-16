# Contributing to pheobe

pheobe is small on purpose: one binary, no daemon, no path deps, a JSON
contract that adopting harnesses can rely on. Contributions that keep it that
way are welcome — bug reports with a reproducing task file most of all.

This file has two audiences: humans and AI coding agents. Both are welcome —
pheobe *is* an agent, and most of its commits so far were made by agents
working alongside a human. The expectations are the same; where an agent
needs something extra, it is called out below.

## Before you start

1. `README.md` for the shape of the thing; `DESIGN.md` for why it is shaped
   that way.
2. `.jagent/planning/TASKS.md` and `.jagent/planning/tickets/` — most
   non-trivial work already has a `PHEOBE-N` ticket with a problem statement,
   success criteria and explicit non-goals. Read it first; it may have
   already decided what you were about to re-litigate. Field defects live in
   `.jagent/issues/`.
3. `dejavue context` — the recorded decisions and their reasons.
4. If your change is not covered by a ticket or issue, open one before
   writing code (template: `.jagent/planning/templates/ticket.md`).

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

## For AI coding agents specifically

If you are an agent (Claude, Codex, Cursor, opencode, Kimi, Antigravity,
Gemini, Copilot, or otherwise) working in this repo:

- **`CLAUDE.md` and `AGENTS.md` are generated** from `.dejavue/context.md`
  by `dejavue export --target claude|codex --replace`. The begin/end markers
  carry a hash; if either file looks wrong or stale, fix `context.md` and
  regenerate — a hand edit to the generated file is silently overwritten.
- **Announce yourself on the sync channel** before touching anything:
  `PHEOBE_AS=<you> scripts/pheobe-sync read --tail 30`, then `claim` the
  ticket or file, `post` blockers, `done` at handoff. Several agents work
  this repo at once; this is how the last collision was caught (an agent
  numbered its ticket PHEOBE-30 while foreman held PHEOBE-30 — it read the
  channel, renumbered to 31, nothing was lost).
- **Take the next ticket number from the channel and the `tickets/`
  directory together**, not from the board alone — the board lags.
- **Work in a worktree on a branch; never commit to `main`.** The parent
  (whoever dispatched you, or the foreman) merges. Say in your `done` post
  which branch and commit you landed, and whether you pushed.
- **Record real decisions as you make them:** `dejavue decision "<title>"
  --reason "…"` for anything a later reader would otherwise have to
  reverse-engineer from a diff. Mechanical changes do not need this; an
  architectural choice does.
- **Live-verify before reporting done** — the same standard as a human
  contributor, not a lower one. "The tests pass" is not the same claim as
  "I ran the binary and watched it do the thing." When a route needs a model
  or a network you do not have, say exactly that instead of claiming the
  run.
- **Do not invent scope.** If the task turns out to imply a `PHEOBE-N`-sized
  change beyond what was asked, say so on the channel and confirm before
  building it, rather than silently widening the branch.
- **Ground SDK and CLI claims in the source.** Every adapter and adoption kit
  in this repo cites the upstream repository or package version it was read
  against (see `adopt/*/*.md`); a kit written from memory of an API is how
  the cursor kit ended up with `disallowedTools` on a type that has no such
  field (PHEOBE-23).

## Reporting a defect

Open a GitHub issue with the task JSON, the `PHEOBE_*` environment you used,
the handoff report (or the stderr when there was none), and the harness
version if a worker adapter was involved. In-repo, field defects also live as
`.jagent/issues/NN-*.md`.

## License

By contributing you agree your work is licensed under the project's dual
MIT OR Apache-2.0 terms.
