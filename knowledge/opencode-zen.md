---
slug: opencode-zen
name: opencode Zen endpoint
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: true
tags: [opencode, zen, llm, api, gateway, external]
sources:
  - live endpoint: GET https://opencode.ai/zen/v1/models (observed 2026-09-16 — 70 models returned)
  - live probe: POST https://opencode.ai/zen/v1/chat/completions without auth → `{"type":"error","error":{"type":"AuthError","message":"Missing API key."}}`
  - https://opencode.ai/zen (provider landing page)
---

## What it is

**Zen** is opencode's hosted LLM gateway: an OpenAI-compatible inference endpoint
that brokers a rotating catalog of frontier and free models (Claude, GPT, Gemini,
Grok, DeepSeek, GLM, Kimi, MiniMax, Qwen families) under one API key. This entry is
post-cutoff: the endpoint, its model ids, and its auth contract do not exist in any
model's training data — read this, don't recall.

## Wire contract (observed 2026-09-16)

- **Base URL:** `https://opencode.ai/zen/v1`
- **Wire format:** OpenAI **chat-completions** — `POST {base}/chat/completions`
  with `{"model": ..., "messages": [...]}`; standard OpenAI-style request/response
  shapes, including `GET {base}/models` returning an OpenAI-style `{"object":
  "list", "data": [{"id": ..., "object": "model", ...}]}` list.
- **Model ids are BARE — no `opencode/` prefix.** Observed ids (70 total) include
  `claude-opus-4-8`, `claude-sonnet-5`, `gpt-5.4-pro`, `gemini-3.5-flash`,
  `grok-4.6`, `deepseek-v4-pro`, `glm-5.3-flash`, `kimi-k2.7-code`,
  `minimax-m3`, `qwen3.6-plus`, plus `*-free` variants
  (`deepseek-v4-flash-free`, `nemotron-3-ultra-free`, …). Write
  `deepseek-v4-pro`, **not** `opencode/deepseek-v4-pro` — the prefixed form is a
  *different provider's* id convention and does not resolve here.
- **Auth:** HTTP Bearer — `Authorization: Bearer <ZEN API KEY>`. Without a key,
  the endpoint answers `{"type":"error","error":{"type":"AuthError","message":
  "Missing API key."}}`. A Zen key is issued via opencode's own flow
  (opencode.ai/zen); it is **not** any other fleet token.
- **`FLOWNET_TOKEN_OPENCODE` is NOT a Zen API key.** It is the flownet/openko
  identity token for the opencode *runner*; presenting it as the Zen bearer key
  fails auth. Do not wire one into the other's config.

## Gotchas

- **Do not prefix model ids with `opencode/`.** Bare ids only (verified against
  the live `/models` list above).
- **Do not reuse another gateway's id vocabulary.** Different gateways name the
  same family differently (`gpt-5.4-pro` here; prefixed forms elsewhere). Take ids
  from the live `/models` response, not from another provider's docs or from
  memory.
- **Auth errors mean "wrong or missing Zen key," not "bad model."** `AuthError` on
  chat-completions is the auth contract talking; swapping model ids in response to
  it is debugging the wrong variable.
- **The catalog rotates.** Free (`*-free`) and exp models appear and disappear;
  treat any id not present in a fresh `GET /models` as nonexistent rather than
  retrying it.
- **This entry expires fast.** `cutoff_gap: true`, verified against the live
  endpoint on 2026-09-16 — re-verify against `GET /models` before trusting any
  specific model id here after ~90 days.
