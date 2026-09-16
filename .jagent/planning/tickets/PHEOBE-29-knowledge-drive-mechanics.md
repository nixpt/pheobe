# PHEOBE-29 — knowledge drive mechanics: ship the corpus, seed it, inject it

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-29 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W6 — fidelity |
| **Assignee** | foreman (s457) |
| **Dependencies** | PHEOBE-12, PHEOBE-28 |
| **Estimated effort** | S |

## Problem

Found during PHEOBE-28: `knowledge::brief()` emitted only each entry's
name/kind/version/date — the passports' tooling tables and gotchas never
reached the model, and the brief did not even print the entry's path, so
"read the source" was impossible. Separately, `knowledge/` "seeds
`~/.pheobe/knowledge`" only by hand: nothing in the binary carried the
corpus, so a `cargo install pheobe` user had an empty drive and
`pheobe doctor` did not say so (PHEOBE-12 deferred the count).

## Success criteria

- [x] `knowledge::SEED` — the 11 entries compiled in (`include_str!`);
      `seed_list_matches_knowledge_dir` fails the build's tests when a file
      is added without registering it
- [x] `pheobe ctx seed [--force]` writes the corpus to the user drive
      (`$PHEOBE_KNOWLEDGE_DIR` or `~/.pheobe/knowledge`), keeping a user's
      edited entries unless forced
- [x] `Entry.tags` parsed from front matter; `knowledge::repo_signals(repo)`
      reads the repo root (Cargo.toml → rust, go.mod → go, package.json →
      typescript/javascript, CMake/Makefile/*.c → c/cpp, gradle/pom → java/
      kotlin, *.sh or scripts/ → shell, *.sql or migrations/ → sql, *.crush →
      crush, .polydex → polydex, a Zen `PHEOBE_BASE_URL` → zen/opencode)
- [x] `brief(entries, repo)` prints `source: <path>` for every entry and the
      full body for entries whose tags match the repo's signals (pheobe's own
      repo: Rust + Shell bodies, 127 lines total)
- [x] `pheobe doctor` reports the drive: `knowledge: ok (N entries)` or
      `empty — run pheobe ctx seed`
- [x] 4 tests in `src/tests/knowledge.rs`; 133 green; files under the LOC budget

## Non-goals

Ranking or trimming bodies to a token budget (entries are ≤ 90 lines by
CONTRIBUTING rule); learning-store nudges (already separate, `learn.rs`).
