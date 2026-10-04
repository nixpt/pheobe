# Local models for pheobe

pheobe's self mode and ACP server need one thing from an endpoint: an
OpenAI-shaped `/v1/chat/completions` that **renders the `tools` schemas into
the prompt** and returns **structured `tool_calls`**. The loop is plan →
edit → verify → handoff as tool calls; a server that drops either half makes
the model answer in prose and the run dies without a commit.

`scripts/pheobe-local` turns a model you have on disk into that endpoint with
the flags that are known to work, proves it with one probe, and hands pheobe
the env. Everything box-specific (paths, ports, venvs) lives in a registry
outside the repo; the script carries only the engine knowledge.

```sh
scripts/pheobe-local init                 # ~/.pheobe/models.d/ + an annotated example
$EDITOR ~/.pheobe/models.d/gemma4-12b.env # one file per model
scripts/pheobe-local serve gemma4-12b     # start, wait for health, print the env
scripts/pheobe-local smoke gemma4-12b     # renders tools? structured calls?
scripts/pheobe-local run gemma4-12b task.json   # serve if needed → pheobe run
scripts/pheobe-local stop
```

`PHEOBE_LOCAL_DIR` moves the registry and state (`models.d/`, `local/`)
somewhere other than `~/.pheobe`. Everything the script starts is logged to
`local/NAME.log` with the exact argv on the first line.

## The registry entry

`~/.pheobe/models.d/NAME.env` — `KEY=VALUE`, one model per file, no shell
expansion. Unknown keys are refused so a typo can't silently become a default.

| key | engines | meaning |
|---|---|---|
| `ENGINE` | — | `llama` (llama-server) or `vllm` |
| `MODEL` | both | GGUF path (llama) · HF id or local dir (vllm) |
| `ALIAS` | both | served model name — becomes `PHEOBE_MODEL` (default: the entry name) |
| `PORT` | both | default `8081` |
| `CTX` | both | llama `-c` (one slot) · vllm `--max-model-len` (default `16384`) |
| `DRAFT` | llama | MTP/draft GGUF → `-md … --spec-type draft-mtp` |
| `GPU_LAYERS` | llama | `-ngl` (default `99`) |
| `THINKING` | both | `off` (default) passes `enable_thinking=false` through the chat template |
| `TOOL_PARSER` | vllm | `--tool-call-parser` (default `hermes` — see the Qwen3.5 note) |
| `REASONING_PARSER` | vllm | `--reasoning-parser` |
| `GPU_MEM` | vllm | `--gpu-memory-utilization` (default `0.90`) |
| `VLLM_BIN` / `LLAMA_BIN` | — | binaries when not on PATH (a venv's `bin/vllm`) |
| `EXTRA_ARGS` | both | appended verbatim, split on whitespace |
| `NOTES` | — | free text, shown nowhere — for the next person |

## What `serve` runs

**llama-server**

```
llama-server -m MODEL --host 127.0.0.1 --port PORT -ngl 99 -c CTX -np 1 --jinja --alias ALIAS
             [-md DRAFT --spec-type draft-mtp] [--chat-template-kwargs '{"enable_thinking": false}']
```

- `--jinja` is what makes the model's own template render `tools` and parse
  its tool-call markup. Without it the schemas never reach the model.
- `-np 1`: llama-server splits `-c` across slots. A coding harness sends a
  big prompt (opencode's is ~14k tokens); with `-np 2 -c 16384` each slot is
  8k, the request is rejected, and some clients retry that 400 forever.
- `DRAFT`: if the model ships an MTP head (a small sibling GGUF, arch
  `<family>-assistant`), speculative decoding is close to free. gemma-4-12B
  went from single-stream decode to ~137 tok/s at 82 % acceptance on a
  12 GB laptop GPU — faster than a 4B without a draft.

**vLLM**

```
vllm serve MODEL --host 127.0.0.1 --port PORT --served-model-name ALIAS --max-model-len CTX
           --gpu-memory-utilization GPU_MEM --enable-auto-tool-choice --tool-call-parser TOOL_PARSER
           --limit-mm-per-prompt '{"image": 0, "video": 0}' [--reasoning-parser …]
           [--default-chat-template-kwargs '{"enable_thinking": false}']
```

- vLLM renders `tools` via the HF chat template, but the **parser** has to
  match the family's emission format or the call comes back as plain text
  with `tool_calls: []` — the client sees exactly what a server that never
  rendered tools would show. `smoke` tells the two apart.
- Memory on a 12 GB GPU: a 4B bf16 model needs the GPU alone plus
  `EXTRA_ARGS=--enforce-eager --max-num-seqs 2 --max-num-batched-tokens 4096`
  (the CUDA-graph profile pass otherwise OOMs); a 0.8B fits beside a
  llama-server at `GPU_MEM=0.55`.
- First start is slow (torch.compile + graph capture, ~2–3 min); restarts
  with a warm cache take ~25 s.

## The probe: `smoke`

```
finish_reason:    tool_calls
tool_calls:       [{"type": "function", "function": {"name": "get_weather", …
prompt_tokens:    71 with tools vs 24 without  (delta 47)
renders tools:    yes
structured calls: yes
```

Two facts, each with one line of evidence:

1. **renders tools** — the same request with and without `tools` must cost
   more prompt tokens with them. A delta of 0 means the schemas were parsed
   and discarded server-side; the model cannot call what it has never seen.
   (zorro serve at 0.43 does this — its `tools` become a decode grammar only;
   zorro `ZORRO-020` tracks the fix.)
2. **structured calls** — the reply carries `tool_calls[]` with
   `type: "function"`. If the text of a call is sitting in `content` instead,
   the server rendered the tools but is running the wrong parser.

`smoke` exits 0 only when both are `yes`. Run it once per new model or engine
version before blaming a pheobe run.

## Model notes (measured, 2026-09-17, RTX 5070 Ti Laptop 12 GB)

| model | engine | result on pheobe's tasks | note |
|---|---|---|---|
| gemma-4-12B-it Q4_K_XL + MTP head | llama | `multiply`: 9 turns / 7.8 s · new-module `stats.py` with edge cases: 7 turns / 11 s, clean code | best local engine on a 12 GB box; 7.8 GB VRAM |
| Qwen3.5-4B Q4_K_M | llama | `multiply`: 19 turns / 11 s · `stats.py`: correct edits, then ran `git init --bare` in the kitchen | pheobe issue 17; keep to single-file tasks until it lands |
| Qwen3.5-4B bf16 | vllm | `multiply`: 8 turns / 26 s | needs the whole GPU; `TOOL_PARSER=qwen3_xml` |
| Qwen3.5-0.8B | vllm | engine fine; the model mangles a two-file edit over 48 turns | smoke-test model only |
| any GGUF | zorro serve 0.43 | `ok: false` — tools not rendered | not a pheobe endpoint yet (`ZORRO-020`) |

**Qwen3.5 on vLLM emits Qwen3-Coder-style XML** (`<function=…><parameter=…>`),
not Hermes JSON: use `TOOL_PARSER=qwen3_xml` and `REASONING_PARSER=qwen3`.
With `hermes` the call is left in `content` and `smoke` says so.

**Thinking**: pheobe's turns are short and many; leave `THINKING=off` unless a
task needs reasoning — a 4B with thinking on spends most of each turn in
`<think>`, and on llama-server without a reasoning format the block leaks into
`content`.

## Where the worker adapters fit

`PHEOBE_PROVIDER=opencode|claude|codex|kimi|…` does **not** use this
endpoint: each adapter spawns that harness's CLI, which brings its own model
configuration. Only opencode is naturally local — point one of its providers
at the same server (`opencode.jsonc` → `options.baseURL =
http://127.0.0.1:8081/v1`) and select it with `PHEOBE_OPENCODE_MODEL=<provider>/<model>`.
`serve` keeps `-np 1` for exactly that client.

## Sizing rule of thumb (12 GB VRAM)

| weights | fits with | headroom |
|---|---|---|
| ≤ 4 GB (4B Q4) | anything else small | plenty |
| ~8 GB (12B Q4 + draft) | nothing else | 16k context OK |
| ~9 GB (4B bf16 on vLLM) | nothing else | eager mode, 2 sequences |
| 16 GB+ (27B Q4) | — | a pod, not a laptop |

The GPU is single-tenant: `serve` stops whatever entry it started before
starting another, and refuses a port it did not open.
