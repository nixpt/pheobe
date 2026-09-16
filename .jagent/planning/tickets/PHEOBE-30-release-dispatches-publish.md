# PHEOBE-30 — release.yml dispatches publish.yml at the minted tag

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-30 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | W5 — release |
| **Assignee** | foreman (s457) |
| **Dependencies** | PHEOBE-27 |
| **Estimated effort** | S |

## Problem

First live run of the channel (2026-09-16): the PHEOBE-29 `feat:` merge made
release.yml mint v0.2.0 correctly (bump commit pushed, then the tag), but
publish.yml never ran — a tag pushed with `GITHUB_TOKEN` raises no `push`
event. api-drift (the precedent) has the same gap and publishes by hand via
`workflow_dispatch` every time. 0.2.0 was published locally with the vault
key to unblock.

## Success criteria

- [x] release.yml: `permissions.actions: write`; after the bump step,
      `git describe --exact-match` → `gh workflow run publish.yml --ref <tag>`;
      no tag → "nothing to publish"
- [x] docs/RELEASING.md explains the dispatch and records the 0.1.0/0.2.0 history
- [x] proven: the PHEOBE-31/32 `feat(update)` merge → v0.3.0 minted → publish
      dispatched → 0.3.0 published, unattended (token then; OIDC since)

## Non-goals

Upstreaming into `squadron/bin/bump-version --install` (a `ci`/scaffold
ticket there — SQ), or api-drift's copy.
