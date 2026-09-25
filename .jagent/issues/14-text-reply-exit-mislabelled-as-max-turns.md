# A model that stops calling tools is reported as `max_turns (N) reached without handoff`

**Found:** 2026-09-17, foreman s500 (vega, [zorro] box), self-mode run against `zorro serve`
(Qwen3.5-4B Q4_K_M) where the engine — for its own reasons, see zorro `ZORRO-020` — never saw the
tool schemas and answered in prose. The handoff report said:

```json
"blocked": "max_turns (32) reached without handoff",
"usage": { "turns": 3 }
```

Three turns is not thirty-two. The loop in `src/agent.rs` has two `break` paths that are not a
hard stop: the aging/budget ladder (which sets `hard_stop` and is reported correctly), and

```rust
if msg.tool_calls.is_empty() {
    history.push(msg);
    break;
}
```

— a plain text reply with no tool call. That path sets nothing, so the fallthrough arm of the
`handoff` match (`None => … "blocked": format!("max_turns ({}) reached without handoff", …)`)
claims the turn budget was exhausted. `max_turns` is `budget.max_iterations * 8` (`src/run.rs`),
which is where the 32 came from.

**Severity:** P3 — the mechanical side is right (no commit, `ok: false`, the `done_when` gate ran
and failed as it should); only the `blocked` reason lies. But `blocked` is the one field an adopter reads to decide what to do next,
and "budget exhausted → give it more budget" is the wrong next move for "the engine answered in
prose" (which is an endpoint/model problem, not a budget one). The `summary` field in the same
report was the model's prose — a ```json-fenced pseudo-handoff — which made the misdirection
worse: it *looked* like a handoff that ran out of turns.

**Status:** Open

## Reproduction

Any endpoint whose model replies without a tool call. Fastest without a broken server: point
`PHEOBE_BASE_URL` at `llama-server` started **without** `--jinja` (tool schemas are then not
rendered and most models answer in prose), run any task, read `blocked`.

## Expected behavior

`blocked` names the actual exit: e.g. `engine replied with text and no tool call after 3 turns
(no handoff)`, with the prose kept in `summary` as today. `max_turns (N) reached` only when
`turns == cfg.max_turns`.

## Fix sketch

- Set a distinct `hard_stop`-style reason on the text-reply `break` (`no_tool_call — engine
  answered in prose on turn {turns}`), or make the fallthrough arm check `turns >= cfg.max_turns`
  before claiming it.
- `next_steps` for that case: "check the endpoint renders `tools` (compare `prompt_tokens` with
  and without `tools`); try `tool_choice`-strict mode; try another model" — not the budget advice.
- Test: a fake provider that returns a text reply on turn 1 → report's `blocked` mentions the
  text-reply exit and not `max_turns`.
