# pheobe

A coding workhorse with no face. Task in, worktree branch + handoff report out —
a headless agent loop designed to be adopted as a subagent by any harness
(opencode, codex, claude, bro) or by a horse/foreman. See `DESIGN.md`.

## Identity

- **Repository:** pheobe (local, no GitHub remote yet)
- **Language:** Rust (single binary, `src/main.rs`; clap + reqwest, OpenAI-shaped `/chat/completions` client)
- **Protocol:** stdio + exit codes; JSON handoff report on stdout; `PHEOBE_BASE_URL` / `PHEOBE_MODEL` / `PHEOBE_API_KEY` select the model endpoint
- **Ticket prefix:** `PHEOBE-N` (tickets); `.jagent/issues/NN-*.md` for defects found in the field

**Working this backlog?** Read `.jagent/planning/RULES.md` first — one worktree/branch per
ticket + verify-before-fix. pheobe itself refuses to cook in the primary checkout.
