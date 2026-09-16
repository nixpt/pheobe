# Allowlist gate mangles the first `git status --porcelain` line — every real run is blocked

**Found:** 2026-09-16, foreman s456, live run against OpenCode Zen (`deepseek-v4-pro`, `kimi-k2.6`, `glm-5.2` — identical on all three)
**Severity:** P0 — the exit gate rejects every successful model turn
**Where:** `src/worktree.rs` — `run()` (~line 25) and `check_allowlist()` (~line 90-108)

## Steps to reproduce

1. Scratch git repo with `calc.py` (`return a - b`) and `check.py` (`assert calc.add(2,3)==5`), committed.
2. `task.json`:
   `{"task":"fix add()","done_when":{"type":"command","run":"python3 check.py"},"paths_allow":["calc.py"],"budget":{"max_iterations":4}}`
3. `PHEOBE_BASE_URL=https://opencode.ai/zen/v1 PHEOBE_MODEL=deepseek-v4-pro PHEOBE_API_KEY=… pheobe run task.json`

## Expected behavior

The model fixes `calc.py` (it does — worktree shows `return a + b`, `.pheobe/plan.json` has 3/3 steps done, `done_when` passes), the allowlist sees only `calc.py` changed, report `ok: true`.

## Actual behavior

```
{"ok": false, "blocked": "paths outside paths_allow: alc.py"}
pheobe: run blocked by allowlist violations
```

`alc.py` — the leading `c` is gone. `worktree::run()` returns `String::from_utf8_lossy(&out.stdout).trim()`, so porcelain's first line ` M calc.py` becomes `M calc.py`; `check_allowlist` then takes `line.get(3..)` (correct for the untrimmed `XY<space>path` format) and yields `alc.py`. Only the first line is affected (later lines keep their leading space), so the bug hits exactly the case that matters: a run whose first status entry is the edited file.

## Fix sketch

Don't trim porcelain output before parsing (add a `run_raw` or parse in `check_allowlist` from the raw bytes), or parse each line as `XY`, one space, path (`line.split_at(2)` → `path = rest.trim_start()`). Add a test with a single modified file as the first status entry.

## Environment

- pheobe `c828bec` (main), release build
- rustc 1.9x (workspace toolchain), Linux (CachyOS)
