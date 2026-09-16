# PHEOBE-7 — codex worker adapter (`PHEOBE_PROVIDER=codex`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-7 |
| **Priority** | P1 |
| **Status** | Done |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | nixp (PHEOBE-7-codex worktree, agent branch) |
| **Dependencies** | PHEOBE-9 (Worker trait) |
| **Estimated effort** | S (after PHEOBE-4) |

## Problem

Codex integration Surface 2 (DESIGN.md §"Codex + Kimi integration"): the
cleanest sandbox mapping of all four hosts — codex's own
`Sandbox.read_only | workspace_write | full_access` presets are already a
three-tier ladder, and `ApprovalMode.deny_all` is the headless
no-questions mode. Thread/turn model maps 1:1 onto pheobe's loop.

Reference on disk: `/workspace/external/codex-src/sdk/python` (README,
docs/{getting-started,api-reference,faq}.md, examples, src).

## Decision (recorded per ticket): CLI path, not the Python SDK subprocess

The ticket text allowed either surface. Chosen: the **CLI path** —
`src/worker_codex.rs` spawns `<PHEOBE_CODEX_BIN=codex> exec --json
--skip-git-repo-check --sandbox <tier> <prompt>` with cwd = worktree and
parses the `--json` JSONL event stream. Rationale: (a) same posture as the
other worker adapters — one prompt out, one whole run back, engine owns its
loop; (b) no `openai-codex` Python SDK install requirement (the SDK also
pins an `openai-codex-cli-bin` runtime dep — i.e. the CLI is the engine
either way); (c) mayfly's proven argv. The Worker trait is the mockable
seam — all 10 codex_ tests run model-free against a fake binary.

**Follow-up (recorded, not ticketed):** the Python SDK route
(`thread_start(sandbox=, approval_mode=deny_all, cwd=,
base_instructions=) → thread.run(task) → TurnResult`) — worth a ticket
only if resume/fork (`thread_resume`/`thread_fork`) or per-turn steering
becomes a requirement; the CLI gives none of those in one shot.

## Success criteria

- [x] `PHEOBE_PROVIDER=codex` routes the turn loop through the `Worker`
      trait, mockable (worker.rs REGISTRY + `src/worker_codex.rs`). The
      thread/turn model is served headless via `codex exec --json`
      (the `deny_all` posture — `exec` asks no questions): last
      `item.completed` agent_message = final response, `turn.completed`
      usage = TurnResult tokens. ~~Python subprocess~~ → CLI subprocess
      (decision above).
- [x] Sandbox tier mapping (`PHEOBE_SANDBOX`, default moderate; unknown
      tier = clear error at construction, never a silent downgrade):
      strict = `read-only` + allowlist note appended to the prompt
      (pheobe's mechanical gates — paths_allow at commit, done_when —
      stay the real allowlist; a task needing writes must run moderate),
      moderate = `workspace-write`, free = `danger-full-access` + policy
      invariants around the run (no protected branches, paths_allow).
- [x] Token usage from TurnResult feeds the budget: tokens =
      `input + output + reasoning` from the last `turn.completed`
      (cached input is a subset of input), carried in `WorkerOutcome.tokens`
      so `agent::run_worker`'s budget guard and aging ladder (pre/post
      deadline checks → `ttl_exceeded`) wrap it unchanged. The CLI has no
      mid-run kill; expiry cuts off at the after-call check (result
      ignored, run blocked) — mid-run interrupt needs the SDK route.
- [x] `pheobe adopt codex` prints the bash-tool dispatch doc
      (`adopt/codex/README.md`) plus the Python SDK snippet with the same
      task JSON (`adopt/codex/sdk-snippet.md`) — landed in PHEOBE-15.
- [x] Live smoke against the fleet codex binary + existing codex auth:
      PASSED end-to-end. `PHEOBE_PROVIDER=codex pheobe run` on a scratch
      repo (task: create hello.txt, done_when grep, paths_allow
      [hello.txt]) → ok:true, branch `pheobe/…`, commit `fb7cce3`,
      done_when passed, summary lifted from codex's final agent_message.

## Implementation notes

- Adapter-local prompt additions (two notes appended after the shared loop
  prompt; `agent.rs` untouched):
  1. **worker note** (always): the loop prompt's contract asks for a
     `handoff` tool call codex has no analogue for — the note re-states the
     same report object as the final-message contract; `parse_jsonl` lifts
     a final message that is itself a report-contract object into
     `json_tail`, and an *empty* `"blocked"` string is stripped (it means
     "nothing blocks me", not a block — `agent::normalize` honors presence).
  2. **strict note** (strict tier only): the sandbox is read-only; writes
     need moderate.
- Tokens: `input_tokens + output_tokens + reasoning_output_tokens`
  (cached input is reported separately and is a subset of input).
- `usd` stays None — codex reports no cost signal.
- Binary: `PHEOBE_CODEX_BIN` (default `codex` on PATH); missing binary and
  non-zero exit both surface clear errors (spawn context names
  `PHEOBE_CODEX_BIN`; failures carry the stderr tail).
