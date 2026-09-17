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

`done_when` accepts `command`, `files_exist` and `git_diff_matches`; there is
no `manual` — a run never ends on vibes.

## Commands

| command | what it does |
|---|---|
| `pheobe run <task.json\|->` | the loop, self mode; report on stdout |
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
| `PHEOBE_PROVIDER` | `openai` (built-in turn loop) or a worker adapter: `opencode`, `claude`, `codex`, `cursor`, `kimi`, `agy` | `openai` |
| `PHEOBE_SANDBOX` | `strict` \| `moderate` \| `free` (bwrap tiers; env wins over the task's `sandbox`) | `moderate` |
| `PHEOBE_MEMORY` | `none` \| `local` \| `host` (host = a joker-mcp memory store) | `local` |
| `PHEOBE_KEEP_WORKTREE` | `1` / `true` / `yes` — leave a failed-run worktree on disk | unset (tear down on early `run` error) |
| `PHEOBE_<PROVIDER>_BIN` / `_FLAGS` / `_TIMEOUT_SECS` | per-adapter binary, extra flags, wall-clock cap | adapter default |

## Local models

`scripts/pheobe-local` serves a model from your own registry
(`~/.pheobe/models.d/NAME.env`) on llama-server or vLLM with the flags that
make tool calling actually work, proves it (`smoke`: does the server render
`tools`, does it return structured `tool_calls`), and hands `pheobe run` the
env. `docs/LOCAL_MODELS.md` has the why, measured model notes, and sizing.

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
