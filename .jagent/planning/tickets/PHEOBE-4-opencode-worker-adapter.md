# PHEOBE-4 — opencode worker adapter (`PHEOBE_PROVIDER=opencode`)

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-4 |
| **Priority** | P1 |
| **Status** | In Progress |
| **Phase** | M0 — the loop exists; exit gate must pass a real run |
| **Assignee** | unassigned |
| **Dependencies** | PHEOBE-9 (Worker trait) |
| **Estimated effort** | M |

## Problem

The OpenCode integration design (DESIGN.md §"OpenCode integration") names
Surface 2: pheobe keeps the mechanical half of the loop (intake gate,
worktree, aging ladder, `done_when`, allowlist, pathspec commit, report)
and delegates the **turn loop** to opencode (`opencode run --format json`,
or a session against a running `opencode serve` via `--attach` / REST
`/api/session` + `/prompt`). Today only Surface 1 (Zen endpoint as a plain
`OpenAi` provider) is wired; Surface 2 is design-only.

## Reference (read from /workspace/external/opencode — ground truth on disk)

- `packages/opencode/src/agent/agent.ts` — `Agent.Info` schema:
  `{ name, description, mode: "subagent"|"primary"|"all", permission:
  ruleset, model?, tools?, prompt?, options, steps }`.
- `packages/opencode/src/agent/subagent-permissions.ts` — subagent
  sessions inherit parent deny + external_directory rules; `task`/
  `todowrite` denied by default (no recursion for free).
- `packages/opencode/src/tool/task.ts` — Task tool: `subagent_type`,
  `background: true` (async + auto-notify), `task_id` resume.
- `packages/opencode/src/worktree/index.ts` — native worktrees, branch
  `opencode/<name>`, `show-ref --verify` + suffix-on-collision (same
  lesson as .jagent issue 03).
- Server API (probed live, opencode 1.18.31): `/api/health`,
  `/api/session` (POST), `/api/session/{id}/prompt` (POST),
  `/api/session/{id}/event` (SSE), `/api/session/{id}/interrupt` —
  no OpenAI-completions endpoint on the local server; completions only on
  the hosted Zen endpoint. Surface 2's attach path rides the session API.

## Success criteria

- [x] A `Worker` trait exists alongside `Provider` ("one prompt in, the
      whole turn loop runs outside, one result comes back"); the loop
      dispatcher picks provider vs worker from `PHEOBE_PROVIDER`.
      **Satisfied by PHEOBE-9** (merged W1): `src/worker.rs` holds the
      trait + `WorkerOutcome` + `worker_from_env` dispatcher; `agent.rs`
      `run_worker` wraps it with the aging-ladder/budget guards.
- [x] `PHEOBE_PROVIDER=opencode` runs the task through `opencode run
      --format json [-m "$PHEOBE_OPENCODE_MODEL"] [--attach
      "$PHEOBE_OPENCODE_URL" when set] --auto --dir <worktree>`, with the
      protocol envelope as the prompt. Implemented in
      `src/worker_opencode.rs` (PHEOBE-4). NOTE/DEVIATION: `--agent
      pheobe` is deliberately omitted — the def only exists where the
      adopt kit was installed, and the prompt envelope is self-contained;
      argv follows mayfly's proven shape, re-verified against
      `opencode run --help` (1.18.31). Child-level subprocess timeout
      (default 600s, `PHEOBE_OPENCODE_TIMEOUT_SECS`) added so the engine
      process itself cannot hang forever under the ladder.
- [x] Report normalization: engine output is prose; pheobe runs
      `pheobe verify` + mechanical gates and builds the report itself;
      engine JSON (if the last message parses) merges into
      summary/next_steps/doubts only. Adapter extracts `json_tail` from
      the final text (whole-text / ```json fence / widest `{...}` span,
      object-only); `agent::normalize_worker_outcome` (PHEOBE-9) merges
      only summary/next_steps/doubts/blocked and honors ok:false.
- [x] Aging ladder still enforced around a delegated engine (wall-clock
      TTL checked around the `opencode run` subprocess; kill on expiry,
      report `ttl_exceeded` as task-design failure). PHEOBE-9's
      before/after ladder checks in `run_worker` + the adapter's own
      subprocess kill at `PHEOBE_OPENCODE_TIMEOUT_SECS`.
- [ ] `pheobe adopt opencode` prints all three files (self-agent.md,
      host-agent.md, pheobe-host.md) + the `opencode.jsonc` snippet.
      **Partial, satisfied by PHEOBE-15 only per-subcommand**: all three
      opencode files exist (`adopt/opencode/{self-agent.md,
      pheobe-host.md, opencode.jsonc-snippet.md}`), but `pheobe adopt
      opencode` prints only pheobe-host.md and `pheobe adopt
      opencode-self` prints self-agent.md + the snippet. One-line
      main.rs change (have `AdoptCmd::Opencode` print all three) — left
      for a main.rs-touching task to keep this branch's shared-file edits
      minimal.
- [x] Mock-able at the same seam as `Provider` (scripted subprocess or
      trait object) so tests stay network-free. 7 model-free tests
      (prefix `opencode_` in src/tests.rs) drive the real adapter
      through a fake `opencode` shell script that records argv + cwd and
      emits a canned event stream; plus the PHEOBE-9 `ScriptedWorker`
      trait-object seam.

## Resolution (PHEOBE-4, in progress)

Adapter + registry entry landed on branch agent/nixp/PHEOBE-4-opencode.
Build: zero errors/warnings (`cargo check --all-targets`). Tests: 41
pass (34 base + 7 new), repeat-run stable. Live smoke on opencode
1.18.31 (`opencode run --format json --auto "Reply with exactly the
single word: ok"`) produced the exact event shapes the parser handles:
`text` part (`part.text` + `time.end`) → final_text "ok"; `step_finish`
part (`tokens {total, input, output, reasoning, cache{read,write}}`,
`cost`) → sum = tokens.total for that step. Event stream shape verified
from source too (`cli/cmd/run.ts` emit + `session/processor.ts`
step-finish part). Remaining: the `pheobe adopt opencode` one-liner
above. Not merged to main; no push.
