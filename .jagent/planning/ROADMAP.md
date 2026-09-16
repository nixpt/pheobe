# ROADMAP — pheobe

Source of truth for scope is `DESIGN.md`. This file only tracks sequencing.

## M0 — the loop exists ✅ (2026-09-16)

- PHEOBE-1 ✅ the model turn — OpenAI-shaped client + tool-calls loop over the barn
- PHEOBE-2 ✅ aging ladder + budget enforcement
- PHEOBE-3 ✅ exit gate correctness (issues 01–03); PHEOBE-13 ✅ live dogfood on three models

## M1 — adoptable ✅ (2026-09-16)

- PHEOBE-9 ✅ `Worker` trait; adapters 4/5/6/7/8/25 ✅ (opencode, claude, cursor, codex, kimi, agy)
- PHEOBE-15/23 ✅ adoption kits per harness, source-grounded; host mode ✅ (`pheobe verify`, PHEOBE-26 `host setup/finish`)
- PHEOBE-17 ✅ ACP server; PHEOBE-22 ✅ worker-route report fidelity

## M2 — published and self-maintaining ✅ (2026-09-16)

- PHEOBE-18/20/27/30 ✅ release channel: bump on `feat:`/`fix:`, publish over OIDC — crates.io 0.3.0
- PHEOBE-24 ✅ sync channel; PHEOBE-21 ✅ LOC budget; PHEOBE-28/33 ✅ CONTRIBUTING, dejavue, AGENTS.md
- PHEOBE-29 ✅ knowledge drive compiled in; PHEOBE-31/32 ✅ doctor version + `pheobe update`

## M3 — fidelity and proof (current)

- PHEOBE-34/35 coverage tickets; issue 10 ETXTBSY closed (PHEOBE-37); kimi adapter live run; native claude host kit; sandbox-tier mapping table
- Closed-loop learning store graduated from opt-in (`learn.rs`, `PHEOBE_MEMORY=host` proven beyond joker-mcp)
- In-process SDK workers where a harness needs them (cursor `run.steer()` / `getUsage()`) — a `Worker` trait extension, not a rewrite

## Non-goals (from DESIGN.md)

No TUI, no daemon, no HTTP server, no session store; never edits the parent's checkout.
