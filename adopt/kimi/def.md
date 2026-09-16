# pheobe — Kimi Code adoption (self mode, via kimi-agent-sdk)

Kimi Agent SDK is a thin wrapper (Go/Node/Python) over **Kimi Code
(Kimi CLI)** — it reuses the CLI's config, tools, skills, and MCP
servers. The def-based route is Kimi Code's own config; the SDK's
`Config` object or `KIMI_*` env vars pick the provider. Fleet-native
note: this is the same Kimi CLI lineage cece-rs forked from, and the
SDK's `kaos` paths / KAOS sandbox backends (BoxLite, E2B, Sprites) are
the same exec-layer name cece-rs carried into the fleet.

Source-grounded against `/workspace/external/kimi-agent-sdk`:
`README.md` (thin-wrapper model, SDKs table) and
`guides/python/quickstart.md` (env vars, `Config` object, config-file
path, `prompt` / `Session` APIs, `yolo=True`).

## Provider config

Either environment variables:

```bash
export KIMI_API_KEY=your-api-key
export KIMI_BASE_URL=https://api.moonshot.ai/v1
export KIMI_MODEL_NAME=kimi-k2-thinking-turbo
```

or a `Config` object — any OpenAI-shaped endpoint the fleet already
runs can be a named provider:

```python
import asyncio
from kimi_agent_sdk import Config, prompt

config = Config(
    default_model="kimi-k2-thinking-turbo",
    providers={
        "pheobe-zen": {
            "type": "kimi",
            "base_url": "https://opencode.ai/zen/v1",
            "api_key": "your-api-key",
        }
    },
    models={
        "kimi-k2-thinking-turbo": {
            "provider": "pheobe-zen",
            "model": "kimi-k2-thinking-turbo",
        }
    },
)

async def main() -> None:
    async for msg in prompt(task_text, config=config):
        print(msg.extract_text(), end="", flush=True)

asyncio.run(main())
```

A config file path (`Path("config.toml")`) can be passed to `prompt` /
`Session.create` instead. Use the low-level `Session` API
(`Session.create(work_dir=...)` + `ApprovalRequest.resolve("approve")`)
when you need per-turn approval handling; `yolo=True` auto-executes
everything — equivalent to the free tier, use with caution.

## Def-based subagent (Kimi Code config)

Kimi Code's subagent surface is its own config files (same ones the SDK
reuses), so the kit's prompt text (`adopt/claude/self-agent.md`-shaped:
bash-only dispatcher writing the task JSON and running
`pheobe run --json`) registers there unchanged. pheobe's
no-recursive-spawning rule is enforced by the kit text itself — keep
the toolset to bash so no spawn/fanout tool exists.

## Sandbox tier ↔ kimi

| pheobe tier | kimi surface |
|---|---|
| strict | KAOS sandbox backend (BoxLite / E2B / Sprites — `examples/python/kaos`); no such backend reachable → fail closed |
| moderate | default KAOS path with approval handling via `Session` + `ApprovalRequest` (approve only allowlisted shapes) |
| free | `yolo=True` + pheobe policy invariants (safe-exec, paths_allow, no protected branches) |

Degradation rule: host can't provide the tier →
`{"ok": false, "blocked": "sandbox_unavailable"}`.
