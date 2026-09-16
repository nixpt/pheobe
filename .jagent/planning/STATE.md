# Planning state — pheobe

**Updated:** 2026-09-16 (foreman s457)
**Milestone focus:** M2 — proven adoptable and published; now fidelity + coverage

**Branch:** `main` @ crates.io 0.3.0. Public on GitHub since 2026-09-16
(v0.1.0 hand-tagged, v0.2.0 and v0.3.0 minted by `release.yml`, published by
`publish.yml` over OIDC — the whole chain has run unattended once).

**What is real (verified live, same day):** self mode over flownet
(`cc-kimi-k3`) and Zen free tier; host mode with `pheobe host setup/finish`
+ `pheobe verify`; worker adapters claude (live e2e), opencode server layer
(`--attach`, live paid + free), cursor (SDK-grounded, PHEOBE-23), codex
(static SDK check), agy (adapter + kit); ACP server; knowledge drive
compiled in (`ctx seed`, matching bodies in the brief); `pheobe update`.
177 tests, clippy `-D warnings` + fmt clean, `cargo package` in CI. Issue 10
closed (PHEOBE-37: `write_shim` tmp+rename).

**Coverage (cargo-llvm-cov, 2026-09-16, post PHEOBE-34):** 83.16% lines /
81.05% regions. `main.rs` 80.91% (CLI contract tests), `tools/search.rs` 14%
(PHEOBE-35), `memory.rs` 62% (host store), `update.rs` live probe + install spawn.

**Open:** issue 11 (`target*` walk skip); kimi adapter never run live;
kimi adapter never run live; a native claude host kit (gap in DESIGN's kit
table); sandbox-tier mapping table across adapters; free-tier opencode
budget note in the adapter doc.

## Waves (history)

| wave | tickets | outcome |
|---|---|---|
| W0 | PHEOBE-13 dogfood | 3 Zen models `ok:true`; exit-gate defects 01–03 found and fixed |
| W1 | 9, 10, 12, 15 | Worker trait, barn hardening, knowledge seed, adopt kits — 4 parallel worktrees |
| W2 | 4, 5, 7 | opencode / claude / codex adapters, each live-smoked |
| W3 | 14, 11, 6 | sandbox ladder, structural ladder, cursor adapter |
| W4 | 8, 16, 17 | kimi adapter, memory trait swap, ACP server |
| W5 | 18, 20, 27, 28, 30, 33 | release posture, RC audit, remote + channel, docs/dejavue/provenance, publish dispatch, AGENTS.md |
| W6 | 21, 22, 23, 24, 25, 26, 29, 31, 32 | LOC budget, worker-route fidelity (issues 05–09), cursor SDK/CLI alignment, sync channel, agy adapter, host supervisor, knowledge mechanics, doctor version, update |

RULES.md applies recursively: new defects found during any wave get their
own issue; tickets' `Status` flips with a `## Resolution` section. Live
coordination runs on `scripts/pheobe-sync`; several agents (foreman, cursor,
codex, agy, the captain's sessions) work `main` concurrently.
