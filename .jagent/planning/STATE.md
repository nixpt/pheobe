# Planning state — pheobe

**Updated:** 2026-09-16
**Milestone focus:** M0 — the loop exists; exit gate must pass a real run

**Branch:** `main` (PHEOBE-2 in flight on `agent/nixp/PHEOBE-2`)
**Verified live:** 2026-09-16 — `pheobe run` against OpenCode Zen (`https://opencode.ai/zen/v1`)
with `deepseek-v4-pro`, `kimi-k2.6`, `glm-5.2`: model turn + tool loop correct on all three,
every run then blocked by the allowlist gate (issues 01/02).
