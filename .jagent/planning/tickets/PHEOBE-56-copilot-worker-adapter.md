# PHEOBE-56 — GitHub Copilot CLI worker adapter

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-56 |
| **Priority** | P2 |
| **Status** | Done |
| **Phase** | W6 — fidelity (engines) |
| **Assignee** | vega |
| **Dependencies** | PHEOBE-9 (Worker seam), PHEOBE-46 (engine parity) |
| **Estimated effort** | M |

## Problem

The GitHub Copilot CLI (`copilot -p`, npm `@github/copilot`) had no pheobe worker. It could
only run bare, without pheobe's intake gate, worktree, allowlist, `done_when` and report.

## Success criteria

- [x] `PHEOBE_PROVIDER=copilot` (or task `provider: "copilot"`) drives the Copilot CLI in the
      worktree and ends in pheobe's commit (`Pheobe-Task:` trailer) + handoff report.
- [x] The engine's final message feeds `json_tail`; turns and tokens are reported; a failed
      run is an error, not prose.
- [x] Sandbox tiers, `PHEOBE_COPILOT_{BIN,MODEL,FLAGS,TIMEOUT_SECS}`, task `model`/`ttl`
      honoured like every other adapter (PHEOBE-46).
- [x] Live evidence: two passing tasks, a failing `done_when`, the budget guard, and ACP.

## Technical approach

`src/worker_copilot.rs`. argv (Copilot CLI 1.0.98):
`copilot -p <prompt> --output-format json --no-ask-user -C <wt>
--deny-tool=shell(git push) --deny-tool=shell(gh:*) [--model m]
--usage-output-file <wt>/.pheobe/copilot-usage.json <flags>`, flags default
`--allow-all-tools --disable-builtin-mcps`, cwd = the worktree.

- `-p` mode denies (never prompts) any tool needing approval, so `--allow-all-tools` is what
  lets the engine edit files and run `done_when`. Copilot's path verification keeps file
  tools to cwd (= `-C` = the worktree) + temp; `--allow-all` would lift that, so it is not
  the default.
- `--deny-tool` beats every allow rule. `git push` and every `gh` call are always denied
  (fixed args, not flags): pheobe never pushes or opens PRs, and the engine inherits a GitHub
  token. Live: a bare `shell(gh)` did NOT deny `gh --version`; `shell(gh:*)` does.
- The JSONL stream (`{"type":..,"data":..}` lines, final `{"type":"result","exitCode":..}`)
  has no token counts, so `--usage-output-file` writes them into the worktree's
  git-excluded `.pheobe/` (survives the moderate tier's private `/tmp`), read then deleted.
- final text = last non-empty `assistant.message` content; turns = `assistant.turn_end`
  count; tokens = `tokenDetails` input + cache_read + cache_write + output; usd = none
  (Copilot bills premium requests / AI credits; no price table invented — codex precedent).
- `strict` refused (hosted model), `moderate` = pheobe bwrap with `~/.copilot` writable and
  `COPILOT_HOME` bound when redirected (PHEOBE-47), `free` = plain.

## Verified

- 10 tests in `src/tests/copilot.rs`: registry; argv shape; parser (last message, turns,
  json_tail, `exitCode`/error events, prose fallback); usage-file tokens; fake-binary runs
  (argv/cwd/env model wins/usage read + sidecar removed; `PHEOBE_COPILOT_FLAGS` replaces the
  default but not the deny rules; failed run surfaces the error event; crash without events;
  strict refused without spawning).
- Live (Copilot CLI 1.0.98, model auto → `gpt-5.6-luna`, GitHub Codespace, no usable bwrap so
  moderate degraded to plain):

  | task | outcome | commit | wall |
  |---|---|---|---|
  | add `multiply` + test | ok, exit 0, 9 turns | `aedcb2b` | 26.7 s |
  | `stats.py` mean/median/mode + edge-case tests | ok, exit 0, 10 turns, 9 tests | `7eaf524` | 35.0 s |
  | unsatisfiable `done_when` (`uname -s = Plan9`) | ok:false, exit 1, blocked verbatim | `99a5194` | 31.4 s |
  | budget guard (`max_usd` 0.0001, `PHEOBE_USD_PER_MTOK=1`) | ok:false, `budget_exceeded — ~$0.19 spent (187447 tokens)` | `e9a267a` | — |
  | ACP `pheobe acp --stdio`, task `provider: "copilot"` | ok, `end_turn`, report as final chunk | `565e383` | 28.1 s |
  | `files_exist` outside `paths_allow` | **ok:true — wrong**: engine committed `build/release.bin` | `f1a2721` | 75.4 s |

  The last row is engine-agnostic and filed as issue 18 (the allowlist gate only checks the
  dirty tree, never the engine's own commits) — not fixed here.

## Non-goals

- An adoption kit for hosting pheobe *inside* Copilot CLI (`adopt/copilot/`) — that is the
  host direction; this ticket is the worker direction.
- Copilot's native `--sandbox` (MXC): it wraps only shell commands and needs slirp4netns +
  iptables; pheobe's bwrap is the tier, as for claude/opencode.
- A USD estimate from premium requests / AI credits.
