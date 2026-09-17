# Echoed tool_calls lacked `"type": "function"` — llama.cpp's server rejects the history

**Found:** 2026-09-16, foreman s457, first pheobe self-mode run against a Kaggle-GPU
`llama-server` (`b1-4bc272f`, Qwen3.8-27B Q4_K_M behind a Cloudflare tunnel):
`endpoint 500: Failed to parse messages: Missing tool call type: {"id":…,"function":…}`
on turn 2, when pheobe replayed the assistant's first `read` tool call.
**Severity:** P1 — every self-mode run against a spec-strict OpenAI-compatible server
died on its second turn. flownet/Zen/opencode gateways tolerated the omission, which is
why 0.1.0–0.3.2 never saw it.
**Status:** Done (PHEOBE-39)

## Resolution

`llm::ToolCall` gains `kind: String` serialised as `"type"`, defaulting to `"function"`
on deserialise (servers that omit it in *responses* still parse). Test
`tool_calls_carry_type_function_on_the_wire`. Re-run on the same endpoint: `ok:true`,
9 turns, 126 s, 10/10 tests, commit `e63b2a8` on `pheobe/kaggle-gpu-cube`.
