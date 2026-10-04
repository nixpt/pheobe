# `llm::OpenAi::chat` sends no `max_tokens` — the reply budget is whatever the server's default is

**Found:** 2026-09-17, foreman s500 (vega, [zorro] box), first pheobe self-mode run against a
*local* engine. `src/llm.rs` builds the request as exactly `{model, messages, tools}` (`tools`
removed when empty) and nothing else — no `max_tokens`, no `temperature`, no `stream`. Every
endpoint pheobe had been run against until now (flownet/Zen/opencode gateways, `llama-server`)
defaults an omitted `max_tokens` to "until stop or context", so the omission was invisible.
`zorro serve` defaults it to `--max-tokens` = **128** per request. A plan/edit turn that writes a
file body is cut off mid-JSON at 128 tokens and the tool call never parses. (The run in question
also failed for a separate reason — see zorro ticket `ZORRO-020` — but this one would have bitten
next.)

**Severity:** P2 — silent, engine-dependent truncation of the exact turns that do the work.
"Works on llama-server, breaks on zorro" is the client's omission, not the engine's; any
OpenAI-shaped server with a conservative default reproduces it (OpenAI's own legacy
`/v1/completions` defaulted `max_tokens` to 16 — small defaults are not exotic).

**Status:** Open

## Reproduction

1. `zorro serve <any GGUF> --host 127.0.0.1 --port 8082` (leave `--max-tokens` at its default).
2. `PHEOBE_BASE_URL=http://127.0.0.1:8082/v1 PHEOBE_MODEL=x pheobe run task.json` with any task
   whose edit is longer than ~100 tokens of file content.
3. Watch `.pheobe/` turn log: the assistant's tool-call arguments end mid-string; pheobe reports
   the call unparseable or the model "re-plans" the same step.

Cheaper: `zorro serve … --request-log` and confirm each `/v1/chat/completions` reply carries
`completion_tokens: 128` exactly.

## Expected behavior

pheobe owns its reply budget. The request carries an explicit `max_tokens` large enough for a
whole-file `write` argument (a few thousand tokens; the value should be derived from, or capped
by, the task's own budget so a runaway turn can't eat the TTL), and the endpoint's default never
decides how long a turn may be.

## Fix sketch

- `llm::OpenAi::chat`: add `"max_tokens": <n>` to the body. Default ~4096; `PHEOBE_MAX_TOKENS`
  env override for endpoints with a small context. Keep it out of the request only when the env
  explicitly asks (`PHEOBE_MAX_TOKENS=none`) for servers that reject the field.
- Test: the serialised body contains `max_tokens` (same shape as
  `tool_calls_carry_type_function_on_the_wire`).
- `docs/`: note the env in the endpoint table next to `PHEOBE_BASE_URL`/`PHEOBE_MODEL`.
