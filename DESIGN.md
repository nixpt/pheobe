# pheobe — Design

> A coding workhorse with no face. Task in, worktree branch out.

**Status:** implemented — v0.1.0, `cargo install pheobe` (crates.io-ready
metadata, no path-deps). Remote: `nixpt/pheobe` (GitHub). The contract below
is the shipped behavior, ratified by 100 green tests and live dispatches
via bro's ACP (`bro synapse dispatch -- pheobe acp --stdio`).

## Problem

The workspace has two shapes of agent and a gap between them:

- **bro / cece-rs** — full agent harnesses with faces: TUIs, personas, CNS/organ
  layers, fleet planes, ACP servers for humans. Great when a human drives.
  Heavy when another *agent* just needs a scoped piece of engineering done.
- **mayfly** — a policy wrapper around *other* harnesses. Owns task validation,
  TTL, and teardown, but has no loop of its own; it delegates all thinking to
  a spawned cursor/claude/codex process and watches.

What is missing: **a real agent loop that is headless end-to-end** — no TUI,
no server, no session store — whose whole lifecycle is designed around being
*adopted as a subagent* by any harness (opencode, codex, claude, bro) or by a
horse/foreman. One binary: task in, branch + report out.

## Position in the stack

```
captain / human
      │
  foreman / horse            ← multi-step, personas, merge waves
      │
  pheobe                     ← THIS — scoped coding task, own loop, own worktree
      │
  kitchen / buckets worktree ← isolation + commit + handoff primitive
```

And from the side of the adopting harness:

```
opencode agent def ┐
claude subagent md ┤
codex bash tool    ┤   self mode:  pheobe run --task <…> --json ──► handoff report
bro synapse        ┤   host mode:  kit prompt runs on the adopter's own LLM; pheobe verify gates the exit
any shell          ┘
```

pheobe is a peer to mayfly, not a replacement: mayfly wraps *harnesses*;
pheobe *is* the worker a harness can adopt. mayfly can later grow a
`harness: "pheobe"` id and spawn pheobe like any other runner — the task
schemas are deliberately kept compatible in spirit.

## Core rules

1. **Own loop.** pheobe plans, implements, tests, debugs, and verifies itself.
   It never asks its parent mid-task; it reports at the end or on failure.
2. **No face.** No TUI, no daemon, no HTTP server. Stdio and exit codes only.
   (ACP stdio is the one exception — it *is* stdio.)
3. **Never cooks in the parent's kitchen.** All work happens in a worktree
   (`kitchen enter` / `buckets worktree`), on a fresh branch. The parent's
   checkout is never touched.
4. **Handoff is structured.** The stable contract is a JSON report on stdout.
   Branch name, worktree path, test results, summary, next steps.
5. **Scoped by construction.** One task per run. No recursive spawning of
   agents. Same fuzziness gate mayfly uses: vague asks die at intake.
6. **Persona and skills are data, not code.** A persona file + skills dir
   shape its behavior; a repo can override them via `.pheobe/`.
7. **Two execution modes, one contract.** The loop can be driven by pheobe's
   own binary + endpoint (`self` mode), or by the adopting harness's own LLM
   (`host` mode, mayfly-style inversion — see "Execution modes"). The JSON
   handoff report and the loop stages are identical in both.

## Aging ladder

> Ported from mayfly, *inside* the loop — which is what MAYFLY-2 (aging
> inject) left parked: pheobe owns its turns, so the injects are real.
> *(Wired as of PHEOBE-2: `src/aging.rs`, `LoopCfg { ttl, max_usd,
> usd_per_mtok }`.)*

| life used | state | behavior |
|-----------|-------|----------|
| 0–50% | alive | normal work |
| 50–75% | warn | inject "narrow to done_when only" once |
| 75–100% | narrowing | inject "final push — verify or hand off" once |
| 100% | expired | hard stop; report `ttl_exceeded` |

A run that hits its deadline without done_when is a **task-design failure**
(mayfly doctrine), reported as such with a re-scope next_step — never as
"ran out of time". Budgets: `max_iterations` maps to max_turns (×8 turns
per planned iteration), `max_usd` enforces when a price signal exists
(`PHEOBE_USD_PER_MTOK`); without one the USD cap is advisory and the turn/
ttl guards still bound the run.

## The loop

```
intake → orient → plan → implement → verify → iterate → commit → handoff
```

| stage | what happens |
|---|---|
| intake | parse task JSON / args; run the fuzziness gate; refuse vague asks |
| orient | read the relevant files (grep/glob/symbols), `dejavue context` boot packet if `.dejavue/` exists, read AGENTS/CLAUDE.md conventions |
| plan | produce a step list; persist to `.pheobe/plan.json`; expose it as a live todo tool so steps get checked off as done |
| implement | edit files inside the worktree; formatter runs after write/edit (bro's BRO-93 pattern) |
| verify | run the task's tests / build / lints; add tests when the task warrants them |
| iterate | on failure: debug, amend plan, retry — bounded by budget/max iterations |
| commit | conventional commit(s) on the worktree branch |
| handoff | push branch (optional), emit JSON report, exit 0/1 |

The loop is a plain turn loop (uno `step()`-shaped, see below) — no
interactive state machine, no session resume. A pheobe run that dies is
re-run from scratch; the plan file is the only durable intermediate state.

## Loops & graphs engineering — surveyed, mostly declined

Workspace survey (runes `RuneGraph`, contextgc `recall_graph`, polydex
`traverse.rs`, crush-visuals `VisualGraph`/`ExecutionTrace`, flownet,
event-horizon, holon/nexus/foreman): **the fleet has no workflow engine or
task-DAG executor to reuse, and no project models agent loops as state
machines.** Findings and decisions:

1. **The loop stays a linear spine.** It has exactly one back-edge
   (verify → iterate → plan) — modeling that as a state graph or workflow
   engine would be over-engineering. The aging ladder (PHEOBE-2) is
   already the formalized state machine the loop needs: four states,
   deterministic transitions, tested without a clock.
2. **Plan steps stay a linear list, not a DAG.** A single worker with no
   parallelism gets no payoff from dependency edges; `RuneGraph` (the
   fleet's semantic-DAG shape) is a data model without validation
   algorithms — borrowing it would add structure without payoff. If
   parallel step execution ever arrives, the DAG upgrade happens in
   `.pheobe/plan.json`'s schema, not in a new engine.
3. **The one idiom worth copying is polydex's traverse discipline** —
   bounded BFS, visited-set cycle bail with a surfaced `cycle_detected`,
   hard depth caps. pheobe's loop already implements its moral equivalent
   (`max_turns`, the aging ladder's hard stop, the no-thrash rule), so
   nothing to port now; the idiom matters if iterate-stage replanning
   ever grows a graph walk.
4. **Call-graph reuse stays where it is**: blast radius is polydex's job
   (`impact`/`affected-tests` in the read ladder), not pheobe's.
5. **Later, presentation-only:** crush-visuals' `ExecutionTrace` as an
   optional handoff-report artifact (a run as a serializable trace
   payload for the parent's debugger view). Deferred until a consumer
   asks for it.

### Industry check (web, 2026) — the ladder and the decision table

The broader picture confirms the decline and sharpens two points:

- **"Loops are simple graphs"** (LangChain, *3 Years of Graph
  Engineering*; 65M+ monthly LangGraph downloads): a loop is a directed,
  cyclic graph — so a loop-based agent isn't anti-graph, it's the
  simplest rung on the prompt → context → harness → loop → graph ladder.
  The industry decision table agrees: *"one task, one verifier, same
  retry path → single loop — topology overhead wins."* That is pheobe's
  exact shape. Graphs earn their cost when responsibilities diverge,
  finish criteria differ per node, or fan-out/fan-in + approval gates
  are normal — which is **the parent's** domain (foreman dispatching
  several pheobes), not pheobe's.
- **"Coding agents as nodes inside a larger graph is a newly practical
  pattern"** (LangChain) — pheobe is designed to be exactly that node:
  self mode is one worker-callable unit; the parent owns topology. This
  validates the mayfly-style no-recursive-spawning rule.
- **Plan-DAG upgrade path, concretized:** the Graph Harness paper
  (arXiv 2604.11378) — a scheduler-theoretic formalization separating
  planning/execution/recovery with immutable plan versions and a bounded
  escalation protocol — positions static-DAG execution for *"engineering
  tasks where the dependency structure can be articulated upfront."*
  pheobe adopts the paper's vocabulary, not its engine: `.pheobe/
  plan.json` may grow optional `depends_on` per step (validated by the
  plan_tracker tool, cycle-checked with polydex's idiom) so a parent
  that fanned out several pheobe nodes can consume pheobe's plan as a
  sub-DAG. Wave-based parallel execution of steps is explicitly out of
  scope for a single worker.
- **Steer, don't re-architect:** OpenCode-GraphAgent (a DAG-of-child-
  agents plugin for opencode) and the weardo/harness pattern (planner +
  generator + evaluator waves, circuit-breaker stagnation detection,
  worktree-isolated parallel fleet) are parent-side orchestrators —
  pheobe's counterpart obligations stay: honest verdicts in the report
  (`ACCEPT`-grade evidence: test output, diffs), bounded iteration
  (aging ladder ≈ their circuit breaker), and clean resume semantics
  (branch + plan.json ≈ their checkpoint).

## Task schema (v0)

```json
{
  "task": "Add --json output to `foo report` and cover it with a test",
  "done_when": {
    "type": "command",
    "run": "cargo test -p foo report -- --exact",
    "expect_exit": 0
  },
  "repo": "/path/to/repo",
  "worktree": true,
  "branch": "pheobe/foo-json",
  "ttl": "45m",
  "budget": { "max_iterations": 8, "max_usd": 1.0 },
  "paths_allow": ["src/report.rs", "tests/"],
  "sandbox": "moderate",
  "push": false
}
```

- `done_when` reuses mayfly's kinds (`command`, `files_exist`,
  `git_diff_matches`; `manual` stays forbidden). It is the verify stage's
  success oracle, not just a watcher's exit condition — pheobe *drives*
  toward it rather than polling it.
- `worktree: true` (default) shells out to `kitchen` when present, else
  `buckets worktree`; falls back to a plain `git worktree add` if neither is
  on PATH. Never edits the given repo's checkout (same warning mayfly emits).
- `paths_allow` is enforced at intake (plan must only touch allowed paths)
  and again pre-commit.

## Handoff report (the stable contract)

```json
{
  "ok": true,
  "task": "…",
  "branch": "pheobe/foo-json",
  "worktree": "/path/to/repo-pheobe/foo-json",
  "commits": ["abc1234 …"],
  "tests": { "ran": "cargo test -p foo report", "passed": true },
  "summary": "Added --json flag to report; covered by tests/report_json.rs",
  "next_steps": ["foreman: ready for review/merge of pheobe/foo-json"],
  "doubts": ["assumed serde 1.x derive API — no ctx entry for serde; read from vendored source"],
  "usage": { "turns": 14, "usd": 0.42 }
}
```

Adopting harnesses should only ever consume this shape. Everything else
(dialogue, tool calls, plan churn) is logging on stderr / `.pheobe/`.

## Execution modes

The loop stages, worktree isolation, and handoff report are fixed. What is
*not* fixed is whose LLM drives the turns:

### `self` mode — pheobe's own engine

```
adopter ──spawn──►  pheobe run --json  ──►  own endpoint (OpenAI-shaped / uno feature)
```

pheobe is the process: it runs intake→handoff on its own model endpoint.
Full autonomy, needs `PHEOBE_BASE_URL`/`_MODEL`/`_API_KEY` (or `--features
uno` on the fleet). This is the default mode of `pheobe run`.

### `host` mode — the adopter is the engine (mayfly-style inversion)

```
adopter's LLM ──drives──►  pheobe protocol  (spawned inside the adopter's harness)
```

Here pheobe contributes no LLM at all — exactly like mayfly adapters, where
the spawned harness (cursor/claude/codex) supplies the model and mayfly
supplies the contract. The adopt kits ship the loop as a **prompt protocol**:

- the persona file + the six-stage loop (orient/plan/implement/verify/
  iterate/handoff) rendered into the subagent's system prompt / agent `.md`
- the plan-tracker discipline (a plan file, steps checked off)
- the `done_when` verify step as a bash command the subagent must run
- the handoff report as the exact JSON shape to end with

The `pheobe` binary stays useful in host mode as a *verifier* only:
`pheobe verify <task.json>` runs the `done_when` gate + path-allowlist check
and exits 0/1 — so a host-mode subagent can end its turn with a mechanical
check instead of vibes (the same "mayflies don't end on vibes" rule, one
subcommand instead of a watcher loop).

| | `self` | `host` |
|---|---|---|
| LLM provider | pheobe's endpoint | the adopting harness's model |
| binary involvement | full loop | kit install + `pheobe verify` |
| fits | foreman/horse dispatch, mayfly `harness: "pheobe"`, plain CLI | opencode agents, claude subagents, codex, bro — anywhere the parent's model is already paying |

Host mode is the cheap default for harness adopters (no endpoint config, one
prompt file); self mode is the standalone posture (install the binary, it
works anywhere). Kits declare their mode; a harness can install both.

## Model endpoint

Decision (2026-09-16): **own OpenAI-compatible client now, uno behind an
opt-in cargo feature later.** *(Status: the model turn is WIRED as of
PHEOBE-1 — `llm::Provider` trait with a blocking OpenAI-shaped impl
(`PHEOBE_BASE_URL`/`PHEOBE_MODEL`/`PHEOBE_API_KEY`), the turn loop is
mock-provider-tested end-to-end.)*

- v0: minimal reqwest client over the OpenAI chat-completions wire format
  (`/v1/chat/completions`, streaming, tool_calls). Works with the endpoints
  the fleet already runs: opencode server, pipefish `/v1`, Ollama, flownet,
  any OpenAI-shaped gateway. Config: `PHEOBE_BASE_URL` / `PHEOBE_API_KEY` /
  `PHEOBE_MODEL`, plus `~/.pheobe/config.toml` (same env-overrides-config
  shape bro uses).
- Later: `--features uno` wires `uno::ChatProvider`/`step()`/`Toolset`
  (path-dep `../uno`) for anthropic/gemini/kimi providers without the
  OpenAI shim. Same opt-in posture as `buckets`' `buildsched` feature:
  default build stays self-contained; `cargo tree` shows no uno.

Toolset in v0 = pheobe's own built-in tools only; MCP is deliberately out
(v0 non-goal) — a subagent shouldn't need its own servers.

## Sandboxing ladder (self + host modes)

Sandboxing is per-task and laddered, not binary — but **"free" is still
restricted** (no world run). The invariant across all tiers: the worktree
is the whole writable world; the destructive guard and `paths_allow` are
never off; protected branches are never pushed.

```
task.sandbox: "strict" | "moderate" | "free"     (default: moderate)
PHEOBE_SANDBOX                                    (env override, wins)
```

| tier | enforcement | network | bash | degradation when no bwrap/buckets |
|---|---|---|---|---|
| **strict** | `buckets run` (bwrap): mount ns + PID ns, toolchain dirs read-only binds, worktree read-write only, everything else invisible | **off** — deps must be vendored/locked; anything missing is a doubt | allowlisted shapes only (done_when, test/build commands) — the model proposes, the ladder matches | **fail closed**: `blocked: "no_sandbox"` — never a silent downgrade |
| **moderate** (default) | same bwrap binds as strict, plus `buckets`' network-on path (package registries reachable) | on | any command inside the worktree, destructive-guard scanned | process capsule (bro's `process` tier: env-cleared subprocess, cwd-jailed) + a `doubts` note saying containment dropped |
| **free** | policy only, no namespace — but never unrestricted: safe-exec scan always on, root jail (absolute paths escaping the worktree are refused), `paths_allow` at tool level + pre-commit, no protected branches, no source-checkout writes | unrestricted | unrestricted (guarded) | n/a — this tier has no dependency to lose |

The invariants that hold in every tier (what "free" still means):

1. no world writes — outside the worktree nothing is written, ever
2. destructive scan never off (the 2026-05-15 rule; `PHEOBE_SAFE_EXEC_MODE`
   only *loosens* the response to a deny hit under explicit choice, the
   scan itself always runs)
3. `paths_allow` at tool level and pre-commit
4. git operations bounded: no protected branches, no force-push, no history
   rewrite
5. root jail on every path the model touches (self mode: `tools::resolve`;
   host mode: the kit's path rule)

### Self vs host

- **Self mode** — pheobe's binary owns exec, so the ladder is implementable
  directly: strict/moderate shell out to `buckets run`'s bwrap shape (same
  argument order as bro's `sandbox.rs`, which already proved the pattern),
  free runs the guarded subprocess. A strict-mode task that can't be
  sandboxed fails closed with `blocked: "no_sandbox"` — the intake gate,
  not a runtime surprise.
- **Host mode** — pheobe *cannot* sandbox the host's own tools, so the
  ladder becomes a **requested tier that maps to host capabilities**:
  claude → permission modes / tool allowlists; codex → `--sandbox
  workspace-write` (strict/moderate) vs `--sandbox danger-full-access`
  only for explicit free; opencode → tool gating + its own permission
  prompts. The kit states the mapping and the degradation rule: if the
  host can't provide the requested tier, the subagent reports
  `{ok:false, blocked:"sandbox_unavailable"}` instead of improvising.
  `pheobe verify` as a subprocess inherits whatever sandbox the host
  applies to bash — it needs no special casing.

Non-goal (v0): wasm/cvm tiers. bro's `BwrapLevel` ladder shows the shape
(full unshare + cap-drop, new-session + sandbox hostname) — pheobe adopts
it if a consumer ever demands kernel-grade tiers, but for a scoped worker
bwrap + policy is the honest ceiling.

## Tool barn

pheobe's own toolset for self mode — deliberately a *subset* of what cece-rs
and bro already prove out, not a new invention. Both codebases share `uno`'s
tool shapes, so porting is mechanical (cece: `CallableTool2` + schemars; bro:
`Tool::new(name, desc, json_schema)` + dispatch — either compiles into
pheobe's own client's tool registry).

### Borrowed from cece-rs (`cece-agent/src/tools/`)

| tool | what to take |
|---|---|
| `file/read.rs` | ReadFile with size/line limits — the guardrails are the value |
| `file/replace.rs` | exact-string replace — the safest edit primitive for an unattended loop |
| `file/write.rs` | write with diff preview; the approval gate collapses to allowlist enforcement in headless mode |
| `file/glob.rs`, `file/grep.rs` | rg-backed search — direct port |
| `shell.rs` | **output truncation** — the one headless-critical piece (approval gating drops out) |
| `todo.rs` | the tracked-plan skeleton → becomes `plan_tracker` |
| `test.rs` | run tests, parse structured pass/fail instead of raw terminal text |
| `repomap.rs` | PageRank over the crush-symbols DB with `focus_files`/`focus_symbols` + token budget — the best "read the relevant files" orient tool in the fleet |
| `think.rs` | thought logging → cheap, aids post-mortem reports |

### Borrowed from bro (`bro-agent/src/tools/`)

| tool | what to take |
|---|---|
| `edit.rs` | `str_replace`, `insert_content`, `multi_str_replace`, `view_diff` — the editing surface |
| `test.rs` | `test_run` + `test_affected` — `crush_symbols` transitive caller impact → run only affected tests; directly serves the verify/iterate stages |
| `format.rs` | format-on-write (BRO-93): rustfmt/gofmt/prettier after each write/edit |
| `worktree.rs` | `provision_worktree` → wraps kitchen/buckets |
| `git.rs` | status/diff/commit for the commit stage |
| `dejavue.rs` | repo memory intake (`.dejavue/` boot packet) |
| `guardrails.rs` / `write_policy.rs` | paths_allow enforcement at tool level |
| `destructive_command.rs` | the destructive-bash guard — keep; it's a plain matcher, no organs needed |

### pheobe v0 barn

| tier | tools |
|---|---|
| core | read, write, edit (str_replace/insert/multi), glob, grep, ls, bash (truncation + destructive guard) |
| loop | plan_tracker, verify (done_when + test_run parsing), worktree (provision/ship), git (status/diff/commit), format-on-write |
| optional | polydex (skeleton/enclosing/callers/impact/affected-tests), repomap (orient), dejavue_context, **code-atlas edit** (structural write protocol) — skipped, not failed, when their backend is absent |
| **excluded** | subagent/multiagent (no recursion), ask_user (headless — a blocked task becomes `{ok:false, blocked:true}` in the report), dmail/squad/notify, organs (maya/vision/interactd), mcp, compact (a scoped run shouldn't live long enough to need compaction; revisit if runs exceed ~100k tokens) |

## Host-mode tool & skill passthrough

In host mode pheobe's protocol runs *inside* the adopter's harness, so it
inherits the host's body: the host's file/bash/web tools and its own skills
system. The kit must be written so the loop is portable across bodies:

1. **Tools are delegated, not duplicated.** The host-mode kit never
   re-describes file editing or shell running; it refers to "the host's file
   tools / shell tool" and adds only the *discipline* the host lacks:
   the plan file convention, the `done_when` verify step, the JSON report
   format. A host's own tools stay first-class (its permissions, its
   approval gates, its display blocks).
2. **Capability tiers with explicit fallbacks.** Each loop stage lists a
   preferred path and a degraded path: worktree stage → `kitchen enter` →
   `buckets worktree` → plain `git worktree add`; orient stage → host
   symbols/repomap → plain grep. If a stage can't be executed at all
   (e.g. no isolation primitive available), the subagent reports
   `{ok:false, reason:"no_isolation"}` instead of improvising.
3. **pheobe ships skills as markdown into the host.** `pheobe adopt --host`
   installs the persona + loop protocol as host skills/agents
   (`.claude/skills/…`, opencode agent `.md`, etc.), so the host's own skill
   loader picks them up and the host's model runs pheobe's discipline with
   host's tools. Repo-local `.pheobe/` overrides work the same way in both
   modes — same files, two renderers.
4. **Host skills stay visible.** A host-mode pheobe runs *with* the host's
   installed skills (buckets-usage, mom's-kitchen doctrine, whatever the
   harness already teaches), not in a sandboxed prompt bubble. The kit only
   adds; it never strips.
5. **`pheobe verify` is the shared exit gate.** In either mode the turn may
   end mechanically: self mode runs it internally; host mode's subagent
   calls it as a bash tool before emitting the report. Same task JSON, same
   exit semantics.

## Language & framework competence

pheobe works across many languages. The canon's virtue #1 (know before you
build) applies doubly here: a language is a *passport*, not trivia — the
durable facts are its tooling surface, and even those must be confirmed
from the repo on disk, not recalled.

### Language passports (skill `language-passports`)

One skill entry per language, encoding the tooling surface a competent
builder knows — never assuming versions, always reading what's on disk:

| passport | build | package mgr | test | format / lint | version truth |
|---|---|---|---|---|---|
| Rust | cargo (+ workspace) | cargo | `cargo test` | rustfmt / clippy | `Cargo.toml` + `Cargo.lock` on disk |
| Python | — (uv, poetry, pip) | uv/pip | pytest | ruff / black | `pyproject.toml`, `uv.lock` |
| Go | go toolchain | go mod | `go test` | gofmt / `go vet` | `go.mod` |
| TS/JS | tsc / vite | npm/pnpm/bun | vitest/jest | biome/prettier/eslint | `package.json` + lockfile |
| C/C++ | make / cmake / meson | vcpkg/conan | ctest / googletest | clang-format / clang-tidy | `CMakeLists.txt` |
| Java/Kotlin | gradle / maven | same | junit | ktlint / spotless | `build.gradle*` / `pom.xml` |
| Shell | shellcheck-managed | — | bats | shfmt / shellcheck | shebang |
| SQL | — | migrations tooling | fixture-based | sqlfluff | schema files |

The passport skill's rule: before running any build/test command, read the
repo's manifest and **use the lockfile versions as ground truth**, not the
model's memory of what version the ecosystem is on.

### Specialized skills (beyond language)

| skill | scope |
|---|---|
| `test-runner-craft` | per-runner output parsing (cargo/jest/pytest/go) — feeds `verify`; includes "read the actual failure text before proposing a fix" |
| `build-system-craft` | detecting/repairing build systems; sibling path-deps must stay resolvable from worktrees (the `BUCKETS_WORKTREE_DIR` lesson) |
| `literate-code-organizing` | module boundaries, naming (Dijkstra), dependency direction — wired to `say-it-twice` |
| `dep-craft` | reading changelogs + semver; when adding a dep, read its docs from the vendored/installed source, never recall |
| `shell-and-git-craft` | the git/gitops rules pheobe obeys (never main/master/dev, worktree doctrine) |
| `docs-craft` | AGENTS.md / CLAUDE.md / README conventions — orient stage reads them before planning |

## Knowledge drive (anti-cutoff)

**Problem:** frameworks and tooling change fast; every model (and every
adopting host) has a knowledge cutoff. A confident guess about an API that
changed last quarter is worse than an admission of ignorance.

**Borrowed from the squad:** jokersquad's `ctx` tool — a curated,
freshness-tracked, citable *world model* of subjects a model cannot know:

- `internal` — our own projects; never in any training set, however new
- `external` — libraries/tools released or changed after the cutoff
- `reference` — a clone we keep to *learn* from; ground truth is the clone
  on disk — read it, don't recall it

Entries are version-controlled markdown with front-matter (`slug`, `kind`,
`version`, `last_verified`, `verified_by`, `sources`, `repos`,
`cutoff_gap: true`), go stale after 90 days, and — the key property —
**brief** into a compact block injected at the top of the prompt with the
preamble: *where this conflicts with what you "know", this is right and
you are wrong. If a detail isn't here, read the source. Do not guess an
API into existence.*

**pheobe's version:** a `pheobe ctx` subcommand over the same schema, two
scopes:

- `~/.pheobe/knowledge/` — global drive (languages, tools, frameworks);
  the pheobe repo's `knowledge/` dir seeds it, version-controlled with the
  binary's source
- `<repo>/.pheobe/knowledge/` — repo-local entries (pinned framework
  quirks, this repo's internal subject entries — what
  `ctx brief --for-repo` would pick out on a squad box)

Integration points:

1. **orient** runs `pheobe ctx brief --for-repo <worktree>` and prepends
   the block, deduped with the repo's `.dejavue/` boot packet — dejavue
   stays the per-repo *why*; the drive is the world *what*.
2. **ctx as a built-in tool** — the loop calls `pheobe ctx search` mid-run
   whenever it hits a subject it suspects is post-cutoff.
3. **verify/iterate** never consult model memory for API shape — absent a
   fresh drive entry, read the source on disk (installed crate source,
   node_modules, vendored docs). The `reference` kind becomes a standing
   rule, not a convention.
4. **host mode** — unchanged: the kit instructs the subagent to run
   `pheobe ctx brief` the same way. It's prompt data, not a pheobe-only
   mechanism; on a squad box pheobe's drive can *be* `.squad/research`
   via config — same format, one shared research corpus.
5. **stale discipline** — the report carries a `doubts` field noting which
   claims rest on stale or missing entries, so the parent sees exactly
   which facts are cutoff-risky.

## Borrowed: contextgc — budgeted orient + tenured handoff

**contextgc** (sibling crate, used by bro/razor/memory-cli) treats the
context window as a managed heap: events → classify → promote → compact →
tenure → recall/assemble, with liveness-ranked, character-budgeted
assembly (`assemble(&RootSet, budget)`) and append-only JSONL tenure. It
calls no model; the embedder is an injected trait. pheobe borrows it at
two ends of the loop, *without* taking a compaction dependency in v0:

1. **orient = budgeted assembly.** The orient stage is exactly a
   `ContextGc::assemble` call: the root set is (task, done_when,
   paths_allow), the stores are the repo's convention docs, `ctx brief`,
   dejavue boot packet, and repomap/symbols hits. Assembled to the model's
   prompt budget, liveness-ranked (task overlap > recency > access), all
   deterministic — no embedder in v0, lexical matching only. A scoped run
   shouldn't need mid-run compaction *because orient assembled the right
   context in the first place* (that's why `compact` stays excluded).
2. **handoff = tenure, not transcript.** The report's `summary` /
   `next_steps` / `doubts` are a contextgc-shaped promotion: events are
   classified into `Fact` / `Decision` / `Constraint` / `Task` /
   `Question` — the exact `MemoryKind` set — and the parent gets
   tenured records with lineage (which files/commits they cite), not a
   session log to re-read. `record-doubts` (canon virtue #6) is the
   `Question`/`Constraint` kind; `honest-handoff` (virtue #7) is the
   tenure filter itself — only what survives liveness against
   done_when is worth reporting.

Optional later: `--features contextgc` (path-dep) to make tenure writes
durable via `EpisodicStore` JSONL per run, giving parents a replayable
audit trail beyond the summary — same opt-in posture as the uno feature.

## Borrowed: runes — one meaning per field

**runes** (semantic IR for AI reasoning) contributes a *discipline*, not a
wire format, to pheobe's schemas. Three rules, borrowed:

1. **One symbol = one meaning.** The plan file, task schema, and handoff
   report are semantic IR: every field has exactly one semantic operation,
   no synonyms, no polysemy. `branch` means the branch; `doubts` means
   unverified assumptions; `next_steps` means actions for the parent. A
   second consumer (foreman, mayfly, a rune-aware harness) must never
   guess what a field meant — the field's meaning is the contract.
2. **Relationships explicit, grammar implicit.** The report doesn't tell
   a story ("first I tried X, then Y…"); it states relations: commits
   point at files, tests point at the done_when they satisfy, doubts
   point at the claims they qualify. The JSON shape is the graph.
3. **Multiple representations, one graph.** Same semantic content, three
   serializations depending on the reader: human prose (the report's
   `summary`), JSON (the machine contract), and — where the parent is
   rune-aware (bro synapse / flownet / joker_bridge) — a compact rune
   packet per run, translated from the same graph, not authored
   separately. The loop's VERIFY stage *is* the rune `ᚦ`'s VERIFY
   operation applied to `done_when`: check, don't narrate.

v0 implements only the discipline (fixed JSON schema, relation-shaped
report). Rune-packet emission is a later optional feature gated on a
rune-aware parent — no dependency now.

## Borrowed: polydex + code-atlas — the structural read/write ladder

`crush-symbols` was renamed **polydex** (`crush-workspace/polydex`); pheobe
targets polydex and treats the old name as deprecated. **code-atlas**
(design-only, ratified as its own repo consuming polydex as a peer dep) is
the structure-aware write protocol. Together they upgrade pheobe's barn
from text coordinates to structural coordinates:

### Read ladder (orient) — polydex when the index is fresh

| pheobe stage | text tool (always available) | structural tool (when `polydex` on PATH) |
|---|---|---|
| orient / find files | glob | `polydex index` report + `hotspots` |
| understand a file | read + grep | `polydex skeleton <file>` — signatures-only, one screen |
| find a definition | grep | `polydex find` / `enclosing` |
| check blast radius | grep callers | `polydex callers` / `impact` (transitive) |
| pick tests to run | guess / run all | `polydex affected-tests` (feeds `test_affected` → verify stage) |
| grouped search | grep | `polydex grep` (grouped by enclosing symbol, ranked by coupling) |

Rule: `polydex status` freshness gates the swap — if the index is stale,
pheobe falls back to text tools and *says so in the report* (polydex's own
`maybe_wrap_stale` honesty rule, adopted as pheobe's too).

### Write path (implement) — code-atlas as the preferred edit mechanism

Text-coordinate editing is the #1 failure mode of unattended agent coders:
past a few hundred LOC, line numbers drift and `str_replace` anchors rot.
pheobe's edit tier is a ladder:

1. **code-atlas edit (preferred, when available):** stable region handle
   (`path|lang|kind|qualified` — survives line drift) + two-hash guard
   (region content-hash is the default; whole-file hash under `strict`)
   + parse-gated atomic write (tree-sitter validates the candidate buffer
   before anything lands) + unified diff returned, always. An edit that
   fails revision or parse changes nothing — the machine checks, not the
   model's memory (canon virtue #5).
2. **str_replace (built-in fallback):** exact-string replace with
   uniqueness enforcement.

The code-atlas invariants are pheobe's canon in tool form, adopted
verbatim:

- never apply on revision mismatch → machine-checks
- never resolve ambiguity silently (`AmbiguousSymbol { candidates }`, no
  `matches[0]`) → a blocked task is `{ok:false, blocked:true}` +
  `doubts`, never a best-effort guess (record-doubts, honest-handoff)
- never fake structural success; always return the diff → no alibis
- never escape the root jail → the scope-is-a-contract standing rule

### Convergence at orient: context pack

code-atlas's planned `atlas.context(task)` retrieval pack and the
contextgc-shaped budgeted assembly (above) are the same idea at two
layers — task-anchored, token-budgeted context assembled from repo facts.
pheobe's orient stage consumes whichever exists: code-atlas pack >
polydex index reads > raw text tools, in that order, each skipped (and
noted) when absent. No pheobe-owned indexing, no embedder, no MCP — both
tools are subprocesses on PATH, same skip-don't-fail posture as dejavue.

## Borrowed: joker learning + memory — closed-loop lessons (optional, overridable)

**Source:** exosphere `memory-service::learning` (closed-loop learning,
inspired by hermes-agent) + joker-mcp's `LearningService`/UKS (`kcs`
KnowledgeUnit lifecycle) + `joker_store_fact`/`joker_recall_facts`.

pheobe borrows the **closed-loop** half — sessions, passive events, nudges —
and deliberately leaves the rest:

| borrowed | what | v0 shape |
|---|---|---|
| `learning_sessions` | run lifecycle (start/end/outcome) | `sessions.jsonl` append-only |
| `session_events` | passive capture of every loop stage / tool call | `events.jsonl` |
| `nudge_items` | resurfaced gotchas scoped per repo, injected at **orient** next to the `ctx brief` (ctx = the world *what*, verified markdown; nudges = repo ops *gotchas*, earned by runs) | `nudges.jsonl` + `pheobe learn nudge/nudges` |
| post-session review | events → lessons → nudges | self mode: cheap second pass after the loop (v0.2); **host mode: the parent does it** (it already saw the run) |
| `skill_candidates` | auto-extracted procedures pending human approval | v0.2 — nudges that keep recurring graduate to skill candidates |
| facts store / knowledge graph / user profile | joker's identity memory | **not borrowed** — pheobe keeps no second memory layer; repo knowledge lives in `.dejavue/`, user facts stay in the host's memory |

**Opt-in by default.** Disabled unless `PHEOBE_LEARN=1` or
`~/.pheobe/learning/` already exists (or `PHEOBE_LEARNING_DIR` is set) — a
scoped worker defaults to memoryless; the store is an operator choice. The
v0 store is JSONL, keeping the default build dependency-free; the upstream
SQLite/FTS shape returns if the corpus is ever pointed at the shared joker
DB via config.

**Review provider.** The review pass (events → lessons → nudges) is an LLM
call — same provider as the loop **by default** (upstream uses its shared
`ProviderManager` with `reviewer_model: "default"`), decoupled on demand:
`PHEOBE_REVIEW_BASE_URL` / `PHEOBE_REVIEW_MODEL` override, following bro's
embedder precedent (chat on flownet, review on any OpenAI-shaped endpoint).
In **host mode there is no pheobe LLM**: the parent reviews (it already saw
the run), or the host's own memory does under `PHEOBE_MEMORY=host`.

**Learning produces skill *candidates*, not skills.** Mirroring upstream
`skill_candidates` (born `pending`, only `approve_skill` promotes, repeated
extraction bumps `extraction_count`): the review extracts recurring
procedures from events; a nudge that keeps coming back (same repo, same
gotcha) graduates to a `skill_candidate`; **human approval is the gate**
before it ever enters the barn as a runnable skill. This satisfies the
workspace's skill-creation doctrine (recurrence gate, not one-off vibes)
and persona-authoring (skills are contracts, not moods) without adding a
dependency.

**Host-mode override.** `PHEOBE_MEMORY=none|local|host` (default `local`):
`host` reads/writes the adopting harness's own memory surface
(`joker_store_fact` on a joker box, claude/codex memory when they grow one)
instead of pheobe's files; `none` keeps the run memoryless. The kit
declares the mapping; the store interface is one trait in code so the
override is a swap, not a fork.

## Borrowed: jokersquad host-layer (final sweep)

| borrow | what | where in pheobe |
|---|---|---|
| `safe-exec` + `scan-destructive-code` | scan the command (and any script it points at) before exec; modes `warn`/`deny`/`off`; exit 99 on hard-deny — born from the 2026-05-15 disaster | the bash tool's destructive guard becomes scan-gated exec with the same mode semantics (`PHEOBE_SAFE_EXEC_MODE`) |
| `checkpoint` | git-stash-backed *named snapshot* of current dirty state; never touches the working tree | iterate stage: checkpoint at each plan-step boundary; a failed iteration restores the checkpoint instead of hand-editing backwards |
| `commit-msg-agent-trailer` | commits carry agent-identity trailers | handoff: pheobe commits get a `Pheobe-Task: <id>` provenance trailer |
| `agent-handoff` stand-down gate | reassignment requires prior owner's ack | parent-side doctrine; pheobe respects it by never merging — it ships the branch, the parent merges |
| two-layer identity (role + instance) | `identities/<name>.md` role vs accumulated instance memory | pheobe ships the role only (`persona/pheobe.md`); per-repo instance knowledge is already the repo's `.dejavue/` — a scoped worker keeps no second memory layer |

## OpenCode integration (three surfaces)

Verified on this box: opencode 1.18.31; `opencode serve` (headless, REST
session API); `opencode run --format json --agent <x> -m provider/model
--attach <server>`; repo-local `.opencode/agent/*.md` agents; a
permission model (deny-pattern bash rules, edit globs,
`external_directory` allowlist). Zen (the hosted completions endpoint) is
already the live-proven self-mode provider — the s456 field run used it
(`https://opencode.ai/zen/v1` + `deepseek-v4-pro`/`kimi-k2.6`/`glm-5.2`,
identical behavior on all three).

### Surface 1 — pheobe as client on Zen (live)

```bash
PHEOBE_BASE_URL=https://opencode.ai/zen/v1 \
PHEOBE_MODEL=opencode/deepseek-v4-pro \
PHEOBE_API_KEY=… pheobe run task.json
```

Nothing new to build — this is the default `OpenAi` provider against
opencode's hosted completions endpoint. The reference run: model turn +
tool loop correct on all three Zen models, every run then blocked by the
allowlist gate (issues 01–03, now fixed and re-verified).

### Surface 2 — worker adapter (`PHEOBE_PROVIDER=opencode`)

The mayfly inversion, fully inside pheobe: pheobe keeps the **mechanical
half** of the loop (intake fuzziness gate, worktree ladder, aging ladder,
`done_when`, allowlist, pathspec commit, report) and delegates the **turn
loop** to opencode:

```
intake/worktree/aging   pheobe binary   (policy, gates — never delegated)
turn loop               opencode       (its model + its tools = the engine)
verify/commit/handoff   pheobe binary   (mechanical — never delegated)
```

- CLI shape: `opencode run --agent pheobe -m <model> --format json
  --attach http://127.0.0.1:<port>` — the prompt is pheobe's protocol
  envelope (persona + loop stages + task + done_when + report contract),
  i.e. the host-mode kit's text, authored by pheobe, not hand-written by
  the parent.
- `--attach` is the server integration: one headless `opencode serve`
  instance hosts N pheobe runs as sessions (session-scoped, REST
  `/api/session` + `/prompt` + event stream) instead of N cold CLI boots.
- Report normalization: opencode's own agent loop emits what it emits;
  pheobe runs `pheobe verify` + the mechanical gates and builds the
  handoff report itself — the engine's output is prose, the contract is
  pheobe's. If the engine was told the report shape (host-agent.md), its
  last message's JSON is merged into the report's `summary`/`next_steps`/
  `doubts`; mechanical fields are always pheobe's.

Env: `PHEOBE_PROVIDER=opencode` (default `openai`),
`PHEOBE_OPENCODE_BIN` (default `opencode`),
`PHEOBE_OPENCODE_MODEL` (mayfly's `MAYFLY_OPENCODE_MODEL` precedent),
`PHEOBE_OPENCODE_URL` (attach to a running `opencode serve`).

### Surface 3 — subagent defs (host mode kits) — source-grounded

Read from `/workspace/external/opencode` (reference clone — ground truth
on disk, don't recall it). What opencode actually provides:

- **The subagent surface is the Task tool + the agent registry.**
  `Agent.Info` = `{ name, description, mode: "subagent"|"primary"|"all",
  permission: ruleset, model?, tools?, prompt?, options, steps }`. Agents
  register from config `agent` sections or repo-local
  `.opencode/agent/*.md`; the parent spawns them with the Task tool via
  `subagent_type: "pheobe"`, supports `background: true` (async,
  notified on completion) and `task_id` resume.
- **Subagent sessions can't recurse.** `deriveSubagentSessionPermission`
  (agent/subagent-permissions.ts) inherits the parent's `deny` +
  `external_directory` rules and defaults `todowrite`/`task` to denied
  unless the subagent's own ruleset permits them. pheobe's core rule 5
  (no recursive spawning) is enforced by the host itself when pheobe runs
  as an opencode subagent.
- **Subagents are prompt-driven sessions**, not spawned binaries — which
  is exactly why the self-mode kit is bash-only: the subagent session's
  whole body is "write the task JSON, run `pheobe run --json`, parse the
  report".
- **opencode has native worktrees** (src/worktree): branch
  `opencode/<name>`, generated with `show-ref --verify` + suffix-on-
  collision — the exact lesson PHEOBE-3 just fixed, independently
  confirmed upstream. A later rung in pheobe's worktree ladder when under
  opencode: kitchen > buckets > opencode worktree (server API) > plain
  `git worktree add`.

Two flavors, both `.opencode/agent/*.md` (fields per `Agent.Info`):

| file | mode | content |
|---|---|---|
| `pheobe.md` | self | bash-only toolset, `mode: subagent`; the agent writes the task JSON and runs `pheobe run --json`, then acts on the report — opencode is the dispatcher, pheobe's binary is the engine |
| `pheobe-host.md` | host | opencode's own model runs the pheobe protocol with opencode's own tools (the existing host-agent.md) |

`pheobe adopt opencode` prints both plus the `opencode.jsonc` snippet
(agent entry with a `permission` ruleset + the sandbox mapping below).

### Sandbox tier ↔ opencode permissions

Concrete now that the permission system is read from source: the tier maps
onto the `permission` ruleset an agent def carries (`PermissionV1.Ruleset`
— patterns × actions `allow|ask|deny`), which the subagent session
inherits from the parent plus its own:

| pheobe tier | opencode surface |
|---|---|
| strict | opencode cannot provide namespace isolation → in self mode only, or opencode itself run inside a `buckets run` bwrap (rare; document, don't default) |
| moderate | the `pheobe` agent def's own `permission` ruleset: bash/edit ask-by-default, `external_directory` allowlisted to the worktree + `/tmp` (the shape the fleet's `~/.config/opencode/opencode.jsonc` already uses, minus the workspace-wide allows) |
| free | opencode's default config policy — its deny-pattern bash rules are the same lineage as pheobe's safe-exec list (both descend from the 2026-05-15 rules), so `free` under opencode ≈ its shipped guardrails |

Bonus inherited for nothing: opencode's subagent derivation defaults
`task` to denied, so a pheobe subagent under opencode physically cannot
fan out — core rule 5 enforced at the host layer.

Degradation rule (unchanged from the sandbox ladder section): host can't
provide the requested tier → `{ok:false, blocked:"sandbox_unavailable"}`.

### Ticket

**PHEOBE-4** (planning board): implement Surface 2's worker adapter —
`PHEOBE_PROVIDER=opencode` behind the `Provider` trait's sibling
`Worker` trait (one prompt in, whole-turn-loop out), Zen-vs-local
`--attach` both supported, report normalization, `pheobe adopt opencode`
extended to emit all three files + the jsonc snippet.

## Claude integration (three surfaces — source-grounded)

Reference material on disk: `claude` CLI 2.1.273 on this box,
`/workspace/external/claude-agent-sdk-typescript` (SDK 0.3.273) and
`/workspace/external/claude-agent-sdk-python` (SDK 0.2.153), plus the
`ant` CLI 1.30.0 in `~/.local/bin` (typed Anthropic-API client, Go).

The landscape (per docs + SDK source):

- **Agent SDK** (TS/Python) = Claude Code's agent loop as a library —
  built-in tools, hooks, subagents, sessions, permissions, skills/plugins
  loading from `.claude/` and `~/.claude/`. **The Python SDK ships the
  Claude Code CLI bundled** (`src/claude_agent_sdk/_bundled`,
  `_cli_version.py` = 2.1.273) — the SDK is the CLI wrapped, so the SDK
  surface and the subprocess surface are the same substrate.
- **The documented escape hatch for other languages is exactly pheobe's
  shape:** "run the CLI as a subprocess with the `-p` flag and
  `--output-format json`" (agent-sdk overview). Claude is one of mayfly's
  proven adapters already (`claude -p … --dangerously-skip-permissions`).

### Surface 1 — pheobe as dispatcher-def subagent (native)

Claude's subagent surface is `.claude/agents/<name>.md` (frontmatter:
name, description, tools, model) — the kit ships `adopt/claude/
self-agent.md` (claude spawns `pheobe run --json`, reads the report).
Agent SDK equivalently: subagents defined in code or loaded from the
same `.claude/` files — both routes reach the same def.

### Surface 2 — worker adapter (`PHEOBE_PROVIDER=claude`)

Same mayfly-inversion as the opencode worker adapter: pheobe keeps the
mechanical half (intake gate, worktree, aging ladder, done_when,
allowlist, pathspec commit, report) and delegates the turn loop to
`claude -p <protocol envelope> --output-format json`. Env:
`PHEOBE_PROVIDER=claude`, `PHEOBE_CLAUDE_BIN` (default `claude`),
`PHEOBE_CLAUDE_FLAGS` (default `--dangerously-skip-permissions` — headless
has no approval path; the permit surface is the task's `paths_allow` +
done_when). Aging ladder wraps the subprocess (kill on expiry,
`ttl_exceeded` as task-design failure). Ticketed PHEOBE-5.

### Surface 3 — `ant` as a provider adapter (later)

pheobe's wire is OpenAI-shaped; Claude's Messages API is not. The `ant`
CLI gives a cheap anthropic-shaped client without writing a provider:
`ant messages create --model claude-opus-5 …` as a bridge tool, or a
dedicated `Anthropic` Provider impl later (uno already has one to port —
uno-as-feature). Anthropic auth on this fleet flows through `ccf`/flownet
env — same constraint mayfly documents for its `claude`/`ccf` harnesses.

### Sandbox tier ↔ claude

| pheobe tier | claude surface |
|---|---|
| strict | self mode only (pheobe/buckets own the namespace); SDK-side: a hooks-gated permission profile could approximate, but no namespace — fail closed |
| moderate | `.claude/agents` def with a restricted `tools` list + the permission system (ask-by-default); SDK: permission callbacks |
| free | CLI with `--dangerously-skip-permissions` + pheobe's own policy invariants (safe-exec, paths_allow, no protected branches) |

Degradation rule unchanged: host can't provide the tier →
`{ok:false, blocked:"sandbox_unavailable"}`.

### Ticket

**PHEOBE-5** (planning board): implement Surface 2's claude worker adapter
(`Worker` trait reuse from PHEOBE-4), plus `pheobe adopt claude` extended
to emit the subagent def + the SDK-snippet variant.

## Cursor integration (source-grounded; worker-shaped only)

Reference material on disk: `/workspace/external/cursor-sdks` —
`@cursor/sdk` 1.0.31 (ts-src, unpacked from npm) + `cursor-sdk` Python
1.0.31 (whl + unpacked src) + both doc sources (`docs-*.md`).

**The structural difference from claude/opencode:** Cursor has **no
completions endpoint at all** — their own docs state "the Cursor SDK is
an agent SDK, not a standalone model-inference or chat-completions API";
Router (`auto-smart` + `optimize_for`) exists only for agent runs. So
there is no "pheobe as client of cursor models" surface: cursor is
worker-shaped only. Everything routes through the same two runtimes:

| runtime | what it is |
|---|---|
| local | agent loop **inline in your Node process** (TS) or via a vendored `cursor-sdk-bridge` Node process (Python wheel ships one, bundled node included); files on local disk; model always Cursor-hosted |
| cloud | Cursor-hosted VM, repo cloned in, survives caller disconnect (`bc-<uuid>` agents, `auto_create_pr`) — pheobe's worktree stage is moot there; the contract still applies |

Source facts that shape the design (from the unpacked SDKs + docs):

- **`Agent.prompt()` is pheobe's `Worker` trait verbatim** — one-shot:
  create agent → send → wait → dispose. Local persistence (per-workspace
  store), `Agent.resume()` by id, `run.cancel()`, `run.status`
  running/finished/error/cancelled/expired.
- **Real dollar budgets.** `agent.getUsage()` returns billed token usage
  *and dollar cost* — pheobe's `budget.max_usd` becomes actually
  enforceable on cursor runs without any `PHEOBE_USD_PER_MTOK` price
  signal. The first host where the USD guard is real rather than
  advisory.
- **Steering delivers the aging ladder to a delegated engine.**
  `run.steer(text)` injects into a *running* turn (local only; result
  `complete_delivered` | `revert_to_followup`). Cursor is the one host
  where pheobe's warn/narrow injects can reach a delegated engine
  mid-run instead of only wrapping the black box.
- **Modes map to loop stages**: `mode: "plan"` (explore/plan first) then
  `mode: "agent"` mirrors pheobe's plan → implement boundary.
- **Subagents = `local.agents: Record<string, AgentDefinition>`**
  (`{ description, prompt, model, mcpServers }`); the `task` tool gates
  them, and disabling `task` prevents subagents entirely — no-recursion
  again enforced at the host layer, third ecosystem in a row.
- **Sandbox is first-party**: `local.sandboxOptions.enabled: true` (the
  per-platform `@cursor/sdk-<os>-<arch>` sandbox helper binaries),
  `beforeShellExecution` / `preToolUse` hooks as the policy tier;
  headless default auto-approves tool calls, no human in the loop.
- **`systemPrompt` replacement** (local only, per-account) — pheobe's
  host-mode kit can ride it, but the safer default is the subagent def
  (subagents keep their own prompts).

### Ticket

**PHEOBE-6** (planning board): implement the cursor worker adapter
(PHEOBE-4's `Worker` trait): `PHEOBE_PROVIDER=cursor` (TS via a small
node shim, or Python via subprocess script — the wheel is the easier
subprocess), `CURSOR_API_KEY` auth, `mode="agent"`, sandbox tier =
`local.sandboxOptions.enabled` (strict/moderate) vs hooks (free),
`budget.max_usd` enforced from `getUsage()` (real dollars), aging
injects delivered via `run.steer()` at warn/narrow, `run.cancel()` on
expiry. Plus `pheobe adopt cursor` emitting the `local.agents` def +
hook config snippet.

## Codex + Kimi integration (source-grounded)

References on disk: `/workspace/external/codex-src` (openai/codex,
sparse checkout of `sdk/python` — 5.3M),
`/workspace/external/kimi-agent-sdk` (MoonshotAI, go/node/python — 4.1M),
`/workspace/external/ai-sdk` (Vercel AI SDK providers + harness docs).

### Codex (`openai-codex` Python SDK)

Same substrate pattern as everyone else: the SDK installs a matching
`openai-codex-cli-bin` runtime dependency — the CLI is the engine.
Thread/turn model maps 1:1 onto pheobe's loop:

- `codex.thread_start(approval_mode=, sandbox=, cwd=, model=,
  base_instructions=, developer_instructions=)` →
  `thread.run(task) -> TurnResult` (`final_response`, collected items,
  **token usage**), `thread_resume` / `thread_fork` for resumable runs.
- `Sandbox.read_only | workspace_write | full_access` — per-turn
  overridable (`thread.run(..., sandbox=)`). **Codex has the cleanest
  sandbox mapping of all four hosts** — codex's own filesystem-sandbox
  presets are already a three-tier ladder.
- `ApprovalMode.auto_review | deny_all` (src/openai_codex/
  _approval_mode.py) — `deny_all` is the headless no-questions mode.

Mapping: `PHEOBE_PROVIDER=codex` →
`thread_start(sandbox=Sandbox.workspace_write,
approval_mode=deny_all, cwd=<worktree>,
base_instructions=<protocol envelope>)` → `thread.run(task)` → pheobe's
mechanical gates + report. Tier mapping: strict/moderate =
`workspace_write` (strict additionally with `read_only` turns when
allowlist is tight), free = `full_access` + pheobe policy invariants;
`deny_all` in headless, `auto_review` only when a human babysits.
Ticketed **PHEOBE-7** (S, after PHEOBE-4).

Self-mode adoption stays the bash-tool route (`adopt/codex/README.md`).

### Kimi (`kimi-agent-sdk` — the cece-rs lineage)

Kimi Agent SDK = thin wrappers (Go/Node/Python) over **Kimi CLI (Kimi
Code)** reusing its config, tools, skills, MCP servers — the same Kimi
CLI lineage cece-rs forked from. Python quickstart uses `kaos` paths and
KAOS sandbox backends (BoxLite, E2B, Sprites — examples/python/kaos),
which is literally the same `kaos` exec-layer name cece-rs carried into
the fleet. Config via `KIMI_API_KEY`/`KIMI_BASE_URL`/`KIMI_MODEL_NAME` or
a `Config` object with named providers — meaning a kimi worker adapter
can point at any OpenAI-shaped endpoint the fleet already runs.
Surfaces mirror claude's: worker adapter (Session/stream/approval
handling in code), def-based subagent via Kimi Code's config, sandbox via
KAOS backends. Ticketed **PHEOBE-8** (P3 — last of the worker quartet;
fleet-native since cece-rs already lives here).

### Vercel AI SDK harnesses — industry precedent for the `Worker` trait

`/workspace/external/ai-sdk/` holds the providers page + three harness
adapter docs (codex, claude-code, opencode). Vercel ships
`@ai-sdk/harness` + `@ai-sdk/harness-codex|claude-code|opencode` adapters
(experimental): a `HarnessAgent` connected to each coding agent through a
bridge in a sandbox, streaming harness events over a WebSocket. That is
pheobe's `Worker` trait as an ecosystem standard — validation that the
Worker abstraction is the right seam, and a later *distribution* surface:
pheobe could ship its own `@ai-sdk` harness adapter (like the existing
community `opencode-sdk` provider) so any AI-SDK app can adopt pheobe.
Distribution-level idea; ticket later, after self/host modes are live.

## Persona & skills

The persona is not invented — it is mined from the people who built the
field, distilled into seven virtues each wired to a loop stage (full text:
[`persona/pheobe.md`](persona/pheobe.md)):

| virtue | loop stage | mined from |
|---|---|---|
| know before you build | orient | Knuth (read the pioneers' source), Thompson (hold it in your head) |
| minimal primitives, maximal leverage | plan | Thompson (four calls), Wirth (do no more than necessary), Hamming (transform the hard into the doable) |
| say it twice — informal then formal | implement | Knuth (literate programming; record what doesn't work) |
| taste is removing special cases | implement | Torvalds, Liskov (abstraction that hides complexity) |
| the machine checks everything mechanical | verify | Hopper (don't check by hand), Carmack (measure), Norvig (debug as hypothesis testing) |
| tolerate ambiguity, record the doubts | iterate | Hamming, Darwin (write down contradicting evidence), Stoics (equanimity, no thrash) |
| honest handoff, no alibis | handoff | Hamming (the alibis chapter), Feynman (write down the problem first) |

The persona file carries the voice too: terse, declarative, no performance
of effort — reports state what is true of the code, not how hard the
session worked. Standing rules: scope is a contract ("while I'm here" is
the ulcer that killed Knuth's Volume 2 schedule); compound interest
(Hamming/Bode — one more careful read, every run); the dishes get done
(worktree + branch + report or the run failed); the deadline is self-set
from ttl/budget at intake.

- `~/.pheobe/persona.md` default persona (the canon above), overridable
  per repo by `.pheobe/persona.md`.
- `~/.pheobe/skills/` + repo `.pheobe/skills/` — markdown skill files loaded
  into the system prompt, same convention the workspace already uses for
  personas/skills (jokersquad identities, buckets-usage skill).
- The canon ships as skills, not just prose: `know-before-build`,
  `minimal-primitives`, `say-it-twice`, `remove-special-cases`,
  `machine-checks`, `record-doubts`, `honest-handoff`, `stoic-budget` —
  each is a checkable behavioral rule, not a mood.

## Adoption kits (`adopt/`)

| kit | mechanism | mode |
|---|---|---|
| `adopt/claude/agents/pheobe.md` | Claude Code subagent def; spawns `pheobe run --json`, parses the report | self |
| `adopt/claude/host/agents/pheobe.md` | subagent def embedding the pheobe loop protocol; runs on claude's own model | host |
| `adopt/opencode/agent/pheobe.md` | opencode markdown agent (tool-restricted) | self |
| `adopt/opencode/host/agent/pheobe.md` | opencode agent embedding the loop protocol (opencode agents are exactly this shape — prompt + tool gating, parent's model) | host |
| `adopt/codex/README.md` | no native subagents — bash-tool invocation | self |
| `adopt/bro/` | ACP: `pheobe acp --stdio` ↔ `bro synapse dispatch --` | self |
| `adopt/README.md` | the contract every kit wraps | — |

Each kit is a thin wrapper over the JSON contract; none of them change it.

## CLI

```
pheobe run     <task.json|-|--task "…">  # the loop, self mode; JSON report on stdout
pheobe verify  <task.json>               # done_when + allowlist gate; exit 0/1 (host mode)
pheobe ctx     list|brief|get|search|new|verify   # knowledge drive (borrowed from jokersquad ctx)
pheobe acp     [--stdio]                 # ACP stdio server (bro/ACP adopters)
pheobe adopt   <claude|opencode|codex|bro> [--host]  # print/install that harness's kit
pheobe doctor                              # endpoint + worktree primitive check
```

## Non-goals

- TUI, REPL, daemon, HTTP server (ACP stdio only)
- Long-lived memory, personas-as-identity, bridge presence — pheobe is a
  worker, not a teammate; anything worth remembering belongs in the *repo's*
  `.dejavue/`, not pheobe's head
- Recursive agent spawning (parents fan out; pheobe never does)
- MCP client/server in v0
- Replacing mayfly, bro, or foreman — it slots under all three

## Success metric

A good pheobe run:

1. validates and starts in <1s
2. finishes or reports failure within ttl/budget — never hangs silent
3. touches only `paths_allow`, only in its own worktree
4. leaves the parent's checkout byte-identical
5. hands off a report the parent can act on without reading the transcript
