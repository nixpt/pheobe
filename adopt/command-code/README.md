# pheobe — Command Code adoption kit

Command Code (the `cmd` CLI) adopts pheobe through two of its own extension
surfaces: a **custom subagent** (self mode — pheobe's binary is the engine,
Command Code is the parent) and an **Agent Skill** (the `/pheobe` entry point
a human types, running the same protocol in-session). Both wrap one contract.

Source-grounded against Command Code **CLI 1.54.2** (`cmd --version`) and the
product references the CLI ships for its own agent
(`command-code-knowledge/reference/custom-agents.md`, `…/skills.md`), with the
storage and discovery behaviour verified live through `/agents`,
`cmd skills list --debug`, and a full dispatched run.

| surface | route | installs to | install command |
|---|---|---|---|
| subagent | self (own endpoint, own context window) | `~/.commandcode/agents/pheobe.md` (personal) or `.commandcode/agents/pheobe.md` (project) | `pheobe adopt command-code` |
| skill | self (`/pheobe <task>`, in-session) | `~/.commandcode/skills/pheobe/SKILL.md` (or the project copy) | same kit |

This kit is the two files: `self-agent.md` (the subagent definition) and
`skill.md` (the skill). `pheobe adopt command-code` prints all three.

## Install

The `pheobe` binary must be on PATH (`cargo install pheobe`).

```sh
pheobe adopt command-code
mkdir -p ~/.commandcode/agents ~/.commandcode/skills/pheobe
cp adopt/command-code/self-agent.md ~/.commandcode/agents/pheobe.md
cp adopt/command-code/skill.md      ~/.commandcode/skills/pheobe/SKILL.md
```

Project scope (`.commandcode/agents/`, `.commandcode/skills/`) is committed
with the repo and shared with the team; personal scope applies everywhere.
Both are re-scanned every turn — no restart. The subagent appears under
**Personal** in `/agents`; `cmd skills list` shows the skill, and a skill that
fails to load is reported there rather than failing the session.

Command Code does **not** ship a `pheobe` name itself, so neither surface
collides with a built-in.

## Engine

Self mode needs an OpenAI-shaped endpoint (pheobe is env-driven; there is no
config file):

```sh
export PHEOBE_BASE_URL=…   # any OpenAI-compatible root, usually ending /v1
export PHEOBE_MODEL=…
export PHEOBE_API_KEY=…    # if the endpoint takes one
```

or borrow another harness's engine with
`PHEOBE_PROVIDER=opencode|claude|codex|cursor|kimi|agy`. `pheobe doctor`
reports which is live. With neither, `pheobe run` exits with
`PHEOBE_BASE_URL not set` before spending a turn.

Two wire facts, both found against gateways that validate strictly:

- **Run pheobe >= 0.3.3 on a schema-validating gateway.** Before 0.3.3 an
  assistant message's echoed `tool_calls` omitted `type`, which lenient
  gateways tolerated and strict ones reject on turn 2
  (`400 … messages.N.tool_calls.0.type: expected "function"` — issue 12).
- **Pick model ids the endpoint serves on the OpenAI wire.** A gateway that
  serves some families on the Anthropic wire only answers
  `400 "must be called via …/messages"` for those ids on
  `/chat/completions`. pheobe speaks the OpenAI wire; a 400 here is the
  endpoint, not the task.

## Command Code specifics

- **No-recursion is enforced by the host.** A subagent's tool set cannot
  include `agent`/`agent_output`, so a dispatched pheobe cannot fan out.
  Keep the dispatcher's tools to `shell_command` + `write_file` plus the
  read tools it needs to check a report against the worktree.
- **A subagent auto-runs where the main loop would prompt.** The
  safety-forced prompts still fail **closed** (destructive command, sensitive
  file, outside-workspace write). Write the task JSON into the session
  scratchpad (`$COMMANDCODE_SCRATCHPAD`) or the OS temp dir, and keep
  `paths_allow` inside `repo`, so nothing trips those gates.
- **`plan` mode wins.** A session in plan mode denies the dispatcher's writes
  outright — do not dispatch pheobe from plan mode.
- **One shell invocation.** Each `shell_command` call gets a fresh shell, so
  the `PHEOBE_*` exports and the `pheobe run` must be the same command.

## Sandbox tier ↔ Command Code

pheobe's tier is chosen by the parent session's permission mode; the
dispatcher cannot raise it.

| pheobe tier | Command Code surface |
|---|---|
| strict | `permissionMode: dont-ask` — a fail-closed allowlist: pre-approved calls run, everything else is denied rather than prompting |
| moderate | the session default, with the subagent auto-running and the safety prompts failing closed (the intended serving tier) |
| free | `bypass` (launch flag only: `--yolo` / `--dangerously-skip-permissions`) plus pheobe's own invariants (`paths_allow`, no protected branches, allowlist commit) |

Degradation rule: the host can't provide the tier →
`{"ok": false, "blocked": "sandbox_unavailable"}`. Do not silently downgrade
— and note that `bypass` is deliberately not slash-switchable in Command
Code, so `free` is a launch decision, not a mid-run one.

## The shared JSON contract

Every run ends in the same handoff report — Command Code reads it, pheobe
writes it:

```json
{
  "ok": true,
  "task": "<task>",
  "branch": "<branch>",
  "worktree": "<worktree path>",
  "commits": ["<sha> <message>"],
  "tests": { "ran": "<done_when command>", "passed": true },
  "summary": "<what changed, one paragraph>",
  "next_steps": ["<actions for the parent>"],
  "doubts": ["<unverified assumptions>"],
  "blocked": "<verbatim blocker, only when ok:false>"
}
```

Standing rules across all hosts: one task per dispatch (`done_when` required —
pheobe refuses vague asks at intake); pheobe never touches the source checkout
(worktree ladder: kitchen > buckets > plain `git worktree add`); the report is
the contract, and its mechanical fields are computed by pheobe, never taken
from a model's prose; the **parent merges** — pheobe never merges.

Under `sandbox: moderate` the *dispatched agent's own* `git` calls fail,
because a linked worktree's gitdir sits outside the sandbox mount. pheobe's
commit gate runs outside the sandbox, so commits still land; the failure
surfaces as a `doubts` entry. Verify the branch, not the prose.
