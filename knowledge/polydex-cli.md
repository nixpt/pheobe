---
slug: polydex-cli
name: polydex CLI
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: true
tags: [polydex, code-intelligence, cli, mcp, tooling, external]
sources:
  - live binary: `polydex --help` (observed 2026-09-16 — the full subcommand list below)
  - github.com/nixpt/polydex `README.md` + `AGENTS.md` (the `polydex init`-generated usage block; private repository at time of writing — install from a checkout with `cargo install --path .`)
---

## What it is

**polydex** (renamed from `crush-symbols`, 2026-07-15) is a structural code
intelligence index for polyglot source: tree-sitter symbol extraction (nine
languages: Python, Go, Rust, JS, TS, Bash, C, C++, Crush), SQLite FTS5 + optional
vector hybrid search, call-graph traversal, an inotify watcher, and a CLI. It is a
general-purpose polyglot tool living in `crush-workspace/` only by dependency
shape — it answers "where is X", "who calls this", "what breaks if I change this".
Post-cutoff: it doesn't exist in training data; read this, then the source.

## CLI surface (verified from README, 2026-09-16)

| subcommand | what it does |
|---|---|
| `polydex index [--embed] [--embed-hash]` | build the index; prints an `IndexReport` (files/symbols/edges/embed %, by-language histogram, gaps) |
| `polydex find "<query>" [--hybrid]` | ranked symbol search (FTS5; `--hybrid` adds the vector channel when embeddings exist) |
| `polydex callers <name>` / `polydex callees <name>` | exact call-graph edges |
| `polydex impact <symbol>` | transitive blast radius of a change ("what breaks if I change this?") |
| `polydex trace <FROM> <TO>` | shortest call path between two symbols |
| `polydex affected-tests` | test symbols transitively reachable from any changed symbol |
| `polydex skeleton <file>` | signatures-only view of one file (every symbol: kind, parent, line, in source order) |
| `polydex grep "<regex>" [--in <path>] [--limit N]` | regex over the working tree, hits grouped by enclosing symbol, ranked by coupling |
| `polydex status` | index health + freshness (`fresh` / `STALE (N new, M deleted, K modified)`); prints stamped embedder identity |
| `polydex enclosing <FILE:LINE>` | best enclosing symbol for a line (khukuri inspect reverse-borrow) |
| `polydex hotspots` | call-graph sites hitting the sensitive-callee catalog (eval/exec/…) — structural SAST, reverse-borrowed from khukuri |
| `polydex languages` | registered languages (id, name, extensions, grammar crate) — source of truth for "what does this build index" |
| `polydex reembed --embed-hash` | re-run embeddings without a full reindex |
| `polydex mcp` | newline-delimited JSON-RPC 2.0 MCP server over stdio — 11 `polydex_*` tools (find, callers, callees, impact, trace, affected_tests, enclosing, skeleton, grep, status, index) |
| `polydex init --agents claude\|agents-md` | non-destructive registration: `.claude/skills/polydex/SKILL.md`, `AGENTS.md` marker block, `.mcp.json` (`--dry-run` to preview) |

Also exposed natively as MCP tools (`polydex_*`, 11 of them) via joker-mcp's
path-dep on the crate.

## Gotchas

- **The rename is real: `crush-symbols` → polydex (2026-07-15).** Docs, tickets,
  and muscle memory citing `crush-symbols` are stale — except the env vars below.
- **Known gap: the env vars still carry the old name.** `CRUSH_SYMBOLS_USE_JOKER`,
  `CRUSH_SYMBOLS_EMBED_CMD`, `_URL`, `_MODEL`, `_KIND` were missed by the rename
  (it only matched the lowercase crate-name forms) and are still the live runtime
  knobs. Setting `POLYDEX_*` variants does nothing today.
- **Grouping/freshness asymmetry in `grep`:** hits come from the *live working
  tree*, but symbol grouping comes from the *index* — a stale index can mis-group
  fresh hits. Run `polydex status` first; if `STALE`, `polydex index` to rebuild.
- **Embedder identity is stamped and checked, not assumed.** Dim mismatch turns
  the vector channel *off* (no silent cos=0); model mismatch warns but runs if
  dims match; legacy vectors without meta suggest `reembed`. If `--hybrid` results
  look vector-less, check `status`'s embedder line.
- **`find` without embeddings is FTS5-only.** `--hybrid` helps only if embeddings
  were ever built (`index --embed` / `--embed-hash`); otherwise it degrades to the
  lexical channel.
