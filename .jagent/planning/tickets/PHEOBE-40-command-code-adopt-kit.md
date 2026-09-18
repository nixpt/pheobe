# PHEOBE-40 — Command Code adoption kit (subagent def + `/pheobe` skill)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-40 |
| **Priority** | P2 |
| **Status** | In review |
| **Phase** | M3 — fidelity and proof |
| **Assignee** | nixpt/command-code |
| **Dependencies** | none (needs pheobe >= 0.3.3 for the strict-gateway finding below) |
| **Estimated effort** | S |

## Goal

The kit for Command Code (the `cmd` CLI) — the first adopter whose own
extension surfaces are *both* a first-class subagent layer (a Markdown def
under `~/.commandcode/agents/`, dispatched through the harness's own `agent`
tool) and an Agent Skill (a `SKILL.md`, invocable as `/pheobe`). Every other
kit so far picks one or the other.

## What landed

- `adopt/command-code/README.md` — the kit doc: two install surfaces, the
  engine section, Command Code specifics, the sandbox-tier mapping, and the
  shared contract.
- `adopt/command-code/self-agent.md` — the subagent definition.
- `adopt/command-code/skill.md` — the in-session `/pheobe` skill.
- `src/main.rs` — `AdoptCmd::CommandCode`, printing all three.
- `adopt/README.md` + `README.md` — kit table row, harness list, `adopt`
  command line in the commands table.
- `src/tests/cli.rs` — `adopt_prints_every_kit_and_rejects_unknown` extended
  with the new subcommand.

## What the kit records, and where it came from

Every claim below was observed, not inferred from an API description:

1. **A second strict gateway exists, and pheobe needed 0.3.3 for it.** Issue
   12 (PHEOBE-39) was found on a Kaggle-GPU llama.cpp server; this is an
   independent instance on a *hosted* provider endpoint that validates the
   request schema. pheobe 0.1.0 got `400 … messages.N.tool_calls.0.type:
   expected "function"` on turn 2, because it omitted `type` when echoing the
   assistant's `tool_calls` back. 0.3.3 is correct against both. The kit says
   so, so an adopter on an older build gets a diagnosis instead of a mystery
   400.
2. **No-recursion is host-enforced.** Command Code removes the `agent` tool
   from a subagent's tool set, so pheobe's no-fanout rule is structural here
   rather than prompt discipline (the same guarantee the Claude kit buys with
   `disallowedTools: ["Task"]`).
3. **`plan` mode defeats the dispatcher.** A session in Command Code's plan
   mode denies the subagent's writes outright — documented as a do-not.
4. **A subagent auto-runs where the main loop would prompt**, and the
   safety-forced prompts fail closed. Hence: task JSON in the scratchpad, and
   `paths_allow` inside `repo`.
5. **`moderate` sandbox vs. linked worktrees.** The sandboxed agent cannot
   reach a linked worktree's gitdir, so its own `git` calls fail and the model
   reports that in `doubts` even when the commit landed. The kit tells the
   parent to judge the mechanical fields, not the prose.
6. **The two kit files are copy-ready verbatim** — frontmatter is the first
   line, so `cp` is the whole install. The older kits (`claude`, `agy`) put
   prose and an `## Agent definition` heading above the frontmatter, so their
   install step silently depends on a human extracting the section. Flagged
   for a possible follow-up ticket rather than fixed here.

## Non-goals

- **Host mode.** Command Code could drive `pheobe host setup` / `host finish`
  with its own model. 0.3.3 ships those subcommands, but no host protocol file
  is added until it is live-verified against a real Command Code session — an
  unverified protocol doc is how the cursor kit ended up with
  `disallowedTools` on a type that has no such field (PHEOBE-23).
- Changing any other kit's shape.

## Verification

- `cargo run -- adopt command-code` prints all three files; the contract test
  covers exit 0 + needle.
- The four CI checks (`fmt`, `clippy -D warnings`, `test`, `package`).
- **Live:** a real two-file task dispatched through a Command Code subagent on
  a hosted OpenAI-shaped endpoint (tool calling confirmed
  `finish_reason: tool_calls` first): `ok: true`, branch committed, `done_when`
  green, source checkout untouched, worktree cleaned up. `pheobe verify` exit
  codes confirmed 1 (fail) / 0 (pass) against the same task.
