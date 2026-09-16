# pheobe adoption kits

One kit per harness. `pheobe adopt <subcommand>` prints the kit to
stdout — copy the files it names into the adopting repo. Every kit
installs the same contract; only the wiring differs per host.

| harness | route | file(s) | install command |
|---|---|---|---|
| claude | self (subagent def) | `adopt/claude/self-agent.md` | `pheobe adopt claude` |
| claude | self (Agent SDK, Python) | `adopt/claude/sdk-snippet.md` | `pheobe adopt claude-sdk` |
| opencode | self (subagent def) | `adopt/opencode/self-agent.md` | `pheobe adopt opencode-self` |
| opencode | host (agent def) | `adopt/opencode/pheobe-host.md` | `pheobe adopt opencode` |
| opencode | jsonc registration + sandbox mapping | `adopt/opencode/opencode.jsonc-snippet.md` | `pheobe adopt opencode-self` (combined: self-agent + snippet) |
| codex | self (bash tool) + SDK snippet | `adopt/codex/README.md`, `adopt/codex/sdk-snippet.md` | `pheobe adopt codex` |
| cursor | host (`local.agents` def + hooks) | `adopt/cursor/def.md` | `pheobe adopt cursor` |
| kimi | self (config def + provider env) | `adopt/kimi/def.md` | `pheobe adopt kimi` |

## Install

Make sure the `pheobe` binary is on PATH (`cargo install --path .`).
Then run the kit's install command, and copy/apply the printed files
per the kit's own Install section.

## The shared JSON contract

Every kit ends in the same handoff report — the harness reads, pheobe
(or the host running pheobe's protocol) writes:

```json
{
  "ok": true,
  "task": "<task>",
  "branch": "<branch>",
  "worktree": "<worktree path>",
  "commits": ["<sha message>"],
  "tests": { "ran": "<command>", "passed": true },
  "summary": "<what changed, one paragraph>",
  "next_steps": ["<actions for the parent>"],
  "doubts": ["<unverified assumptions>"],
  "blocked": "<verbatim blocker, only when ok:false>"
}
```

What each field means, exactly:

- `branch` — the branch pheobe worked on. The **parent merges**; pheobe
  never merges (agent-handoff stand-down doctrine).
- `commits` — what landed on that branch, one entry per commit.
- `tests` — the `done_when` command that ran and whether it passed.
  Mechanical, parsed from real output — never narrated.
- `summary` — what changed, one paragraph. No narration.
- `next_steps` — actions for you, the parent.
- `doubts` — pheobe's unverified assumptions; verify them or pass them on.
- `blocked` — when `ok: false`, the reason verbatim. Do not retry
  silently, do not "help" by editing files yourself.

Standing rules across all hosts: one task per dispatch (`done_when`
required — pheobe refuses vague asks at intake); pheobe never touches
the source checkout (worktree ladder: kitchen > buckets > plain `git
worktree add`); the report is the contract; scope is a contract
(`paths_allow` only — "while I'm here" is a bug).
