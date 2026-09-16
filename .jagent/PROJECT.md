# pheobe

A coding workhorse with no face. Task in, worktree branch + handoff report out —
a headless agent loop designed to be adopted as a subagent by any harness
(Claude Code, opencode, codex, cursor, kimi, Antigravity, bro) or by a
horse/foreman. See `DESIGN.md`; `README.md` for the contract.

## Identity

- **Repository:** github.com/nixpt/pheobe (public, `main`; `fleet-default-main-protection` ruleset)
- **Crate:** crates.io `pheobe` — 0.3.0; `cargo install pheobe`, `pheobe update`
- **Release channel:** `feat:`/`fix:` merge → `release.yml` bumps + tags → `publish.yml` (crates.io, Trusted Publishing/OIDC, no stored token). `docs/RELEASING.md`
- **Language:** Rust (single binary, `src/main.rs`; clap + reqwest, OpenAI-shaped `/chat/completions` client; six worker adapters spawn each harness's CLI)
- **Protocol:** stdio + exit codes; JSON handoff report on stdout; `PHEOBE_BASE_URL` / `PHEOBE_MODEL` / `PHEOBE_API_KEY` select the self-mode endpoint, `PHEOBE_PROVIDER` a worker adapter; ACP over stdio (`pheobe acp --stdio`)
- **Ticket prefix:** `PHEOBE-N` (tickets); `.jagent/issues/NN-*.md` for defects found in the field
- **Coordination:** `scripts/pheobe-sync` (jokersquad agent-sync; `.jagent/sync/pheobe.jsonl`, gitignored) — read on arrival, claim before overlapping edits. `docs/SYNC.md`, RULES §7
- **Memory:** `.dejavue/` — `dejavue context` for the boot packet; `CLAUDE.md`/`AGENTS.md` are exports of `.dejavue/context.md`

**Working this backlog?** Read `.jagent/planning/RULES.md` first — one worktree/branch per
ticket, verify-before-fix, LOC budget, the sync channel. pheobe itself refuses to cook
in the primary checkout. Humans and agents: `CONTRIBUTING.md`.
