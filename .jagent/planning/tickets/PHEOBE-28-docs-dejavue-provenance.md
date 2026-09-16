# PHEOBE-28 — CONTRIBUTING, dejavue, and public provenance

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-28 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W5 — release |
| **Assignee** | foreman (s457) |
| **Dependencies** | PHEOBE-27 |
| **Estimated effort** | S |

## Problem

The repo went public (PHEOBE-27) with no CONTRIBUTING, no repo memory, and
docs + the shipped `knowledge/` / `adopt/` corpus citing paths on one box
(`/workspace/external/…`, `/workspace/projects/…`, `~/.local/bin/polydex`,
`.squad/research/…`) as their sources — unreadable to anyone else, and the
passports had no public references at all.

## Success criteria

- [x] `CONTRIBUTING.md`: branch/worktree rule, conventional commits (they
      mint releases), the four CI checks, adapter + knowledge recipes, defect
      reports, license
- [x] `dejavue init` + `context.md` (rules, build, architecture map) + six
      standing decisions with reasons + state snapshot; `.gitattributes`
      union-merge for append-only files; `CLAUDE.md` boot pointer
- [x] every box-local citation replaced by its public upstream: opencode
      (anomalyco/opencode), claude SDKs (anthropics/…, versions), cursor
      (`@cursor/sdk` npm / `cursor-sdk` PyPI 1.0.31, cursor.com/docs/sdk),
      codex (openai/codex `sdk/python`, PyPI `openai-codex`), kimi
      (MoonshotAI/kimi-agent-sdk), Vercel AI SDK; crush → nixpt/crush-ast +
      crates.io versions + nixpt/crush-language-guide; polydex → nixpt/polydex
      (marked private); exosphere named as a private fleet repo, no path
- [x] each language passport cites 2–3 public references (cargo book,
      packaging.python.org, pkg.go.dev/cmd/go, …); `.squad/research`
      citation dropped
- [x] crate excludes `.dejavue`, `CLAUDE.md`, `CONTRIBUTING.md`, `docs/`

## Found, filed as PHEOBE-29

`knowledge::brief()` injects only entry headers — passport bodies never
reach the model — and nothing seeds `~/.pheobe/knowledge` from the shipped
corpus, so a `cargo install pheobe` user has an empty drive.
