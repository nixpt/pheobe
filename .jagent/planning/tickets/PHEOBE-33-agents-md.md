# PHEOBE-33 — AGENTS.md + CLAUDE.md generated from context.md; agents section in CONTRIBUTING

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-33 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W5 — release |
| **Assignee** | foreman (s457) |
| **Dependencies** | PHEOBE-28 |
| **Estimated effort** | S |

## Problem

The repo had a `CLAUDE.md` stub from `dejavue init` and no `AGENTS.md`;
CONTRIBUTING addressed humans only, though most commits are made by agents.
checkstand (`nixpt/checkstand`) is the fleet's pattern: adapter files
GENERATED from `.dejavue/context.md` with a hash-marked block, and a
"For AI coding agents specifically" section in CONTRIBUTING.

## Success criteria

- [x] `.dejavue/context.md` carries the release-channel rule and the
      generated-file rule; `dejavue export --target claude --replace` and
      `--target codex --replace` produce `CLAUDE.md` and `AGENTS.md` with
      identical bodies
- [x] CONTRIBUTING: two-audience preamble, "Before you start" (README →
      DESIGN → board/tickets/issues → `dejavue context` → open a ticket), and
      the agents section: generated files, sync channel on arrival, ticket
      numbers from channel + `tickets/`, worktree + parent merges, record
      decisions, live-verify, no invented scope, source-grounded SDK claims
- [x] `AGENTS.md` excluded from the crate alongside `CLAUDE.md`

## Non-goals

`GEMINI.md` / `.cursor/rules` / copilot instructions — `dejavue export
--target all` produces them on demand; they are not committed until a
contributor on that tool asks.
