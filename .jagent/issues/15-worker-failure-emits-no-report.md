# A worker adapter failure ends `pheobe run` with no report on stdout

**Found:** 2026-09-17, foreman s500 (vega, [zorro] box), first pass over the worker adapters.
Two different failures, same shape:

- `PHEOBE_PROVIDER=opencode` with the subprocess cap hit (`PHEOBE_OPENCODE_TIMEOUT_SECS=480`;
  opencode was retrying a hard 400 from the endpoint in a loop): stdout **empty**, exit 1, stderr
  `pheobe: opencode worker: 'opencode' exceeded the subprocess timeout of 480s (kill applied; …)`.
- `PHEOBE_PROVIDER=kimi` with a CLI that exits 1 at startup (a config error on this box): stdout
  **empty**, exit 1, stderr `pheobe: kimi worker: 'kimi' exited with exit status: 1: …`.

`run_worker` returns `Err`, `run_task` propagates it with `?`, and `main` prints the error and
exits — the handoff report is never built. The self-mode loop has no such path: every exit there
(hard stop, max_turns, text reply) still produces `{ok: false, blocked: …}`.

**Severity:** P2 — the README's first sentence about the report ("the only contract an adopter
should consume") is broken exactly when the adopter most needs it. Every adopt kit tells the host
to "parse the JSON report from stdout" and to "report `blocked` verbatim"; on an adapter failure
there is nothing to parse and the host has to scrape stderr. The worktree is also torn down
(`PHEOBE_KEEP_WORKTREE` unset), so the engine's partial work is gone with it.

**Status:** Open

## Reproduction

```sh
PHEOBE_PROVIDER=kimi PHEOBE_KIMI_BIN=/bin/false pheobe run task.json; echo "rc=$? stdout-bytes=$(pheobe run … | wc -c)"
# or: PHEOBE_PROVIDER=opencode PHEOBE_OPENCODE_TIMEOUT_SECS=1 pheobe run task.json
```

Expected a JSON object on stdout in both cases; observed 0 bytes.

## Expected behavior

An adapter timeout or non-zero exit is a `blocked` outcome, not a crash:
`{"ok": false, "blocked": "<adapter>: <reason verbatim>", "branch": …, "worktree": …}` on stdout,
exit 1, worktree kept (it is not an *early* run error — provisioning succeeded and the engine
may have written files worth inspecting).

## Fix sketch

- In `run_task`, map the worker `Err` into a `WorkerOutcome { ok: false, blocked: Some(msg) }`
  and fall through to the mechanical gates (allowlist / done_when still run on whatever the
  engine left) and the report, instead of `?`-propagating.
- Keep the stderr line (it is useful); the report carries the same text in `blocked`.
- Test: a fake adapter binary that exits 1 → stdout parses as JSON with `ok: false` and
  `blocked` containing the adapter's stderr excerpt.
