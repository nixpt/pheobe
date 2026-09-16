# PHEOBE-11 — structural read/write ladder: polydex + code-atlas integration

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-11 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

The structural read/write ladder (DESIGN.md §"Borrowed: polydex +
code-atlas") is design-only. On this box polydex is on PATH (code-atlas
is not yet built). pheobe must gain the availability-gated tools so a run
on any fleet box upgrades from text coordinates to structural ones.

## Success criteria

- [ ] Optional tools detect `polydex` (accept both old `crush-symbols`
      and new `polydex` binary names; deprecation-safe) and `code-atlas`
      on PATH at barn build time; absent = skipped, never failed.
- [ ] Read ladder: `skeleton <file>`, `find`/`enclosing`, `callers`/
      `impact`, `affected-tests` (feeding the verify stage's
      test-selection), `hotspots` for orient — each wrapped with a
      freshness gate: `polydex status` stale → fall back to text tools
      AND note the fallback in the run's doubts (polydex's
      `maybe_wrap_stale` honesty rule adopted).
- [ ] Edit ladder: code-atlas edit (region handle + two-hash guard +
      parse-gated atomic write + always-return-diff) preferred when
      available; str_replace stays the built-in fallback. Ambiguity →
      `{ok:false, blocked:"ambiguous"}` in the report, never
      `matches[0]`.
- [ ] orient consumes, in order: code-atlas context pack > polydex reads
      > raw text tools — each skipped-and-noted when absent.
- [ ] Tests use a fake `polydex`/`code-atlas` script on a temp PATH to
      prove gate + fallback + stale-note behavior without the real
      binaries.
