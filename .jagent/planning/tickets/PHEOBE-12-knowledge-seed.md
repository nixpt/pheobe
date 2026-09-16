# PHEOBE-12 — knowledge drive seeding: language passports + fleet ctx entries

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-12 |
| **Priority** | P2 |
| **Status** | Done |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | S |

## Problem

`pheobe ctx` (drive scanner + brief) is live but the drive is empty: no
language passports, no external entries. The seeding content is fully
specified in DESIGN.md (§"Language & framework competence", §"Knowledge
drive") — it is data work, ideal for a sub-agent.

## Success criteria

- [ ] `knowledge/` in the pheobe repo (version-controlled, seeds
      `~/.pheobe/knowledge/`) with one passport entry per language:
      Rust, Python, Go, TS/JS, C/C++, Java/Kotlin, Shell, SQL — each
      carrying the passport table row (build, package mgr, test, format/
      lint, version-truth location) + gotchas section.
- [ ] Each entry: front-matter per the ctx schema (`kind: external`,
      `cutoff_gap` set honestly — passports are pre-cutoff material with
      wrong-priority risk, like the squad's `language-implementation`
      entry), `last_verified: <today>`, sources.
- [ ] At least two external `cutoff_gap: true` entries relevant to the
      fleet's day-to-day (candidate: opencode Zen endpoint contract,
      polydex CLI surface) — verified against on-disk source, cited.
- [ ] `pheobe ctx brief` shows all seeded entries fresh; `pheobe doctor`
      counts them.
- [ ] Note in the ticket: the squad's `.squad/research` corpus is
      readable via `PHEOBE_CTX_EXTRA=.squad/research` — the seeding must
      not duplicate entries that already live there (check first).

## Resolution

Merged to main (W1). 10 entries in knowledge/ (8 language passports +
opencode-zen + polydex-cli cutoff-gap entries). Zen contract verified
live (70 models, bare ids, Bearer auth, FLOWNET_TOKEN_OPENCODE is not a
Zen key); polydex surface verified from the live binary (not just
README). No .squad/research duplication (language-implementation.md
cited, not copied). Doctor entry-count deferred (noted). Zero src
changes; 16 tests; ctx list shows all 10 fresh.
