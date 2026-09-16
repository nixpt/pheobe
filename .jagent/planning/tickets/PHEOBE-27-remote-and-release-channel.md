# PHEOBE-27 — remote + release channel: GitHub, release.yml, publish.yml

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-27 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | W5 — release |
| **Assignee** | foreman (s457) |
| **Dependencies** | PHEOBE-18, PHEOBE-20 |
| **Estimated effort** | S |

## Problem

PHEOBE-18 installed the bump kit and held the remote off. Before creating
`nixpt/pheobe` and claiming the crate name (free on crates.io as of
2026-09-16), check the channel end to end: the installed
`scripts/bump-version.sh` (its header still said "on merge to
agent/nixp/PHEOBE-18-release"), `release.yml` against the
`fleet-default-main-protection` ruleset (zorro#128 broke a release this way),
and the missing crates.io leg.

## Findings

- pheobe's kit copy is the post-#128 version (bump commit pushed alone, tag
  only after it lands) — newer than mayfly's copy. No prior `v*` tag →
  the script is a no-op, so the first push to `main` cannot mint a bump;
  v0.1.0 is hand-tagged (kit design).
- `fleet-default` today = `deletion` + `non_fast_forward`, no
  `pull_request` rule and no bypass actors — the release bot's fast-forward
  push is allowed; no deploy key needed (zorro's break was a stricter
  variant).
- No crates.io leg existed. Fleet precedent: `api-drift/publish.yml`
  (Trusted Publishing + token fallback, tag/version match, already-published
  skip). Ported verbatim with the crate name swapped.

## Success criteria

- [x] `scripts/bump-version.sh` header names `main`
- [x] `.github/workflows/publish.yml` on `v*` tags (+ `workflow_dispatch verify_auth`)
- [x] `docs/RELEASING.md`; `docs/` excluded from the crate
- [ ] `gh repo create nixpt/pheobe` + push `main` + `gh-ruleset --policy fleet-default` (captain: visibility)
- [ ] hand-tag `v0.1.0` → publish.yml → crates.io `pheobe 0.1.0` (captain: go + credential)
- [ ] after first publish: configure Trusted Publishing on crates.io, drop the token

## Non-goals

Changing the bump policy; a CHANGELOG generator (the kit deliberately
leaves the narrative to a person).
