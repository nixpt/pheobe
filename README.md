# pheobe

**A coding workhorse with no face. Task in, worktree branch + handoff report out.**

pheobe is a headless coding agent built to be *adopted as a subagent* — by
another agent harness (Claude Code, opencode, codex, cursor, kimi, agy, bro) or by
a human's shell. No TUI, no daemon, no session store. It takes a scoped task,
does the work in its own git worktree on a fresh branch, verifies the result
against a mechanical `done_when` gate, and returns one JSON report. It never
touches the caller's checkout.

```text
adopter ──► pheobe run task.json ──► own LLM endpoint      (self mode)
adopter's LLM ──► pheobe host setup/finish + protocol          (host mode)
```

- **Self mode** — pheobe is the engine: plan → implement → verify → iterate →
  handoff on an OpenAI-shaped endpoint of your choice.
- **Host mode** — the adopting harness's model drives; pheobe provisions the
  kitchen (`pheobe host setup`) and gates the exit (`pheobe host finish` /
  `pheobe verify`).

## Install

```sh
cargo install pheobe            # crates.io
pheobe update                   # later upgrades (same cargo install)
cargo install --path .          # from a checkout
```

Requirements: `git`. Optional: `bwrap` (sandbox tiers), `kitchen` or `buckets`
(worktree primitives — plain `git worktree` is the fallback), `polydex` /
`code-atlas` (structural read/write), `dejavue` (repo memory). `pheobe doctor`
reports what it found.

## Quick start

```sh
export PHEOBE_BASE_URL=http://localhost:4096/v1   # any OpenAI-shaped endpoint
export PHEOBE_MODEL=your-model
export PHEOBE_API_KEY=…                           # if the endpoint needs one

cat > task.json <<'EOF'
{
  "task": "Add --json output to `foo report` and cover it with a test",
  "done_when": { "type": "command", "run": "cargo test -p foo report", "expect_exit": 0 },
  "repo": "/path/to/repo",
  "branch": "pheobe/foo-json",
  "ttl": "45m",
  "budget": { "max_iterations": 8, "max_usd": 1.0 },
  "paths_allow": ["src/report.rs", "tests/"],
  "sandbox": "moderate"
}
EOF

pheobe run task.json        # JSON handoff report on stdout, everything else on stderr
```

The report is the only contract an adopter should consume:

```json
{
  "ok": true,
  "branch": "pheobe/foo-json",
  "worktree": "/path/to/repo-pheobe/foo-json",
  "commits": ["abc1234 …"],
  "tests": { "ran": "cargo test -p foo report", "passed": true },
  "summary": "…",
  "next_steps": ["ready for review/merge of pheobe/foo-json"],
  "doubts": ["…"],
  "usage": { "turns": 14, "usd": 0.42 }
}
```

`done_when` accepts `command` (`{"type":"command","run":…,"expect_exit":0}`)
and `files_exist` (`{"type":"files_exist","paths":[…]}`, relative to the
worktree); there is no `manual` — a run never ends on vibes.
(`git_diff_matches`, advertised here before PHEOBE-44, was never
implemented and is no longer claimed.)

## Commands

| command | what it does |
|---|---|
| `pheobe run <task.json\|->` | the loop, self mode; ONE JSON report on stdout, always — exit `0` ok, `1` ran but `ok:false`, `2` never ran (intake/provision error, reason in `blocked`) |
| `pheobe verify <task.json>` | `done_when` + path-allowlist gate only; exit 0/1 |
| `pheobe host setup <task.json>` | host supervisor: intake + worktree + empty plan; JSON `{ok, worktree, branch, task}` |
| `pheobe host finish <task.json>` | host supervisor: `done_when` + allowlist in `--worktree` (cwd default); JSON; exit 0/1 |
| `pheobe adopt <claude\|claude-sdk\|opencode\|opencode-self\|codex\|cursor\|kimi\|agy>` | print that harness's adoption kit |
| `pheobe acp --stdio` | Agent Client Protocol server over stdio (e.g. `bro synapse dispatch -- pheobe acp --stdio`) |
| `pheobe ctx seed` / `ctx brief --for-repo <dir>` / `ctx list` | knowledge drive: land the built-in corpus in `~/.pheobe/knowledge`, then brief the prompt with the entries that match a repo |
| `pheobe learn …` | closed-loop lesson store |
| `pheobe check …` | named working-state snapshots |
| `pheobe doctor` | version vs crates.io/github tags, endpoint, knowledge drive, worktree primitives |
| `pheobe update` | install a newer pheobe from crates.io (`--git` if unpublished); `--check` / `--force` |

## Configuration

Everything is environment-driven; there is no config file.

| variable | values | default |
|---|---|---|
| `PHEOBE_BASE_URL` / `PHEOBE_MODEL` / `PHEOBE_API_KEY` | the self-mode endpoint | — |
| `PHEOBE_PROVIDER` | `openai` (built-in turn loop) or a worker adapter: `opencode`, `claude`, `claude-sdk`, `codex`, `cursor`, `kimi`, `agy`; wins over the task's `provider` | task `provider`, else `openai` |
| `PHEOBE_SANDBOX` | `strict` \| `moderate` \| `free` (bwrap tiers; env wins over the task's `sandbox`) | `moderate` |
| `PHEOBE_MEMORY` | `none` \| `local` \| `host` (host = a joker-mcp memory store) | `local` |
| `PHEOBE_KEEP_WORKTREE` | `1` / `true` / `yes` — leave a failed-run worktree on disk | unset (tear down on early `run` error) |
| `PHEOBE_<PROVIDER>_BIN` / `_FLAGS` / `_TIMEOUT_SECS` | per-adapter binary, extra flags, wall-clock cap. Every adapter: effective cap = min(this, task `ttl`), and a timed-out engine is killed | adapter default |
| `PHEOBE_<PROVIDER>_MODEL` | per-adapter model (`CLAUDE`, `OPENCODE`, `CODEX`, `CURSOR`, `KIMI`, `AGY`); wins over the task's `model` | unset (task `model`, else the CLI's default) |
| `PHEOBE_CLAUDE_SDK_ALLOW` | claude-sdk: comma list of extra tools to allow outright (e.g. `WebFetch`) | unset |
| `PHEOBE_CLAUDE_SDK_GRACE_SECS` | claude-sdk: seconds to wait for a result after the TTL `interrupt` before killing | `10` |

### Engine parity (PHEOBE-46)

Every engine honours the task's `provider`, `model`, `ttl` and `sandbox` the same
way, and the report carries what the engine actually did:

| engine | model flag | sandbox `moderate` | sandbox `strict` | `usage.usd` |
|---|---|---|---|---|
| `claude` | `--model` | pheobe bwrap | refused (needs network) | reported cost |
| `claude-sdk` | `--model` | pheobe bwrap | refused | `total_cost_usd` |
| `opencode` | `-m` | pheobe bwrap | refused | summed `step_finish.cost` |
| `kimi` | `-m` | pheobe bwrap | refused | if the gateway reports it |
| `agy` | `--model` | pheobe bwrap | agy `--sandbox` | none (not reported) |
| `cursor` | `--model` | cursor `--sandbox enabled` | cursor `--sandbox enabled` | `chargedCents / 100` |
| `codex` | `-m` | codex `--sandbox workspace-write` | codex `--sandbox read-only` | none (no price table invented) |

pheobe bwrap = the WHOLE engine run in bwrap with the network shared, `$HOME`
read-only except the engine's own state dirs (e.g. `~/.claude`,
`~/.local/share/opencode`, `~/.cece`, `~/.gemini`), writes confined to the
worktree + its repo's git store (+ `$CARGO_TARGET_DIR`). `free` = plain subprocess
everywhere. `usage.turns` is the engine's own turn count where it reports one.
An engine that fails **after** committing reports `ok:false` with its commits
(exit 1); one that fails before producing anything is still exit 2.

**Redirected engine config (PHEOBE-47).** When a caller relocates an engine's config/auth dir via env
(agent-launch's per-agent home does), `moderate` mounts it **writable** so the engine still finds its
login: claude/claude-sdk `CLAUDE_CONFIG_DIR`, opencode `OPENCODE_CONFIG_DIR`, kimi (cece) `CECE_HOME` then
`KIMI_SHARE_DIR`; agy reads none; codex/cursor use their native sandbox. Only existing absolute dirs are
bound (never `/`, `$HOME` or an ancestor); a symlink inside one (e.g. `.credentials.json` → `~/.claude/…`)
keeps resolving — its target is mounted read-only unless an existing mount covers it. Each bind is logged
to stderr (`pheobe: <engine> moderate sandbox: binding $VAR=…`).

### The `claude-sdk` worker (PHEOBE-45)

`PHEOBE_PROVIDER=claude-sdk` drives the same claude CLI the way the Claude
Agent SDK does, over its stream-json control protocol
(`--input-format stream-json --output-format stream-json
--permission-prompt-tool stdio --setting-sources=`), with pheobe answering every
permission request. It **never** passes `--dangerously-skip-permissions`.

| tool call | pheobe's answer |
|---|---|
| `Write` / `Edit` / `MultiEdit` / `NotebookEdit` | allow only inside the worktree ∩ `paths_allow` |
| `Bash` | deny destructive commands (safe-exec), `git push`, `gh pr merge`, recursive `rm` outside the worktree, and **shell writes** (`>`/`>>`, `tee`, `cp`/`mv`/`install`/`ln` destinations, `touch`/`mkdir`) outside the worktree ∩ `paths_allow`; otherwise allow |
| `Read` / `Grep` / `Glob` / `LS` / `NotebookRead` | PreToolUse hook: allow inside the worktree or its main checkout, deny elsewhere |
| anything else | deny unless on the allowlist (`TodoWrite`, `BashOutput`, `KillBash`, `KillShell`, + `PHEOBE_CLAUDE_SDK_ALLOW`) |

Every decision is reported: denials land in the handoff's `doubts`, with a
`N allowed, M denied` line. `usage.usd` is the CLI's real `total_cost_usd`,
`usage.turns` its `num_turns`, and `budget.max_usd` becomes `--max-budget-usd`
(an overrun result still blocks). At the TTL, pheobe sends `interrupt`, waits
`PHEOBE_CLAUDE_SDK_GRACE_SECS` for the result, then kills. `--setting-sources=`
keeps the host's `~/.claude` hooks and settings out of the worker.

The Bash write screen is a best-effort parse. Run-level `paths_allow` gating
(the allowlist check after the engine finishes) stays the mechanical backstop
for writes a parse can't see.

**Protocol dependency:** this is the SDK's internal wire contract, not a
documented public API. `tests/fixtures/claude-stream-json.jsonl` (a scrubbed
real transcript) is the conformance test; re-run the suite, and the live test
(`PHEOBE_LIVE_CLAUDE=1 cargo test live_claude_sdk -- --ignored`), when the claude
CLI version moves. The subprocess `claude` worker stays as the fallback.

## Adoption kits

`adopt/` holds one kit per harness, each a thin wrapper over the same JSON
contract: Claude Code subagent defs (self and host), an Agent SDK snippet,
opencode agent defs, a codex bash-tool invocation, a cursor host kit
(SDK `Agent.create` + IDE `~/.cursor/agents/` + CLI `--sandbox` map), a
kimi def, an Antigravity (`agy`) skill and subagent def. `pheobe adopt <name>`
prints the kit; `adopt/README.md` states the contract every kit wraps.

## Design

`DESIGN.md` is the full design: the aging ladder (TTL/budget hard stops), the
six-stage loop, execution modes, the sandbox ladder, the tool barn, what was
borrowed from sibling projects and what was deliberately declined.

## Contributing

`CONTRIBUTING.md` — branch-in-a-worktree, conventional commits (they drive
releases), the four checks CI runs, how to add a worker adapter or a knowledge
entry. Repo memory lives in `.dejavue/` (`dejavue context` for the boot packet).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option.
