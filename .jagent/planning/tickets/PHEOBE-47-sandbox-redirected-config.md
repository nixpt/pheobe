# PHEOBE-47 — moderate sandbox mounts env-redirected engine config dirs

**Filed:** s463 (2026-09-23), foreman. Found by the first live foreman routine-tier dispatch.

## Problem
agent-launch gives each dispatch a common agent home and exports `CLAUDE_CONFIG_DIR`, `CODEX_HOME`,
`OPENCODE_CONFIG_DIR` into it (`.squad/state/agent-homes/<agent>/…`). pheobe's `moderate` bwrap mounted
`$HOME` read-only plus the engine's own `$HOME` state dirs, the worktree and the git store, but not those
redirected dirs, so the engine saw no config. claude-sdk ended with "Not logged in · Please run /login" at
$0.00. Reproduced: the same env with sandbox `free` → ok; `moderate` → Not logged in.

## Fix
`engine::redirected_config` + `Mounts::with_redirects`: per engine, bind the env-redirected config dirs
writable. The env vars were verified against the installed CLIs: claude 2.1.281 `CLAUDE_CONFIG_DIR`; opencode 1.18.32
`OPENCODE_CONFIG_DIR`; cece `CECE_HOME` → `KIMI_SHARE_DIR` (cece/share.py; not `CECE_SHARE_DIR`); agy none;
codex/cursor use their native sandbox. Only existing absolute dirs are bound, never `/` or `$HOME` or an
ancestor of it. The target of a symlink inside a bound dir is mounted read-only unless an existing mount
already covers it. Each bind is logged to stderr.

## Verified
- 8 tests in `src/tests/redirect.rs`, including a real bwrap run where a file under a redirected dir outside
  `$HOME`/`/tmp` is readable and writable, and a control where it is invisible without the bind.
- Gated live test `live_claude_sdk_moderate_with_redirected_config_dir` (PHEOBE_LIVE_CLAUDE=1) passed: haiku,
  moderate, CLAUDE_CONFIG_DIR = the foreman agent home, file written, $0.0215.
- Full pipeline, the exact s463 failing case: `pheobe run`, provider claude-sdk, moderate, redirected
  CLAUDE_CONFIG_DIR → ok:true, commit, 13 turns, $0.0683.

## Not in scope
The claude CLI refreshes its token by write+rename, which replaces a `.credentials.json` symlink in the
redirected dir with a regular file that then goes stale (s463 found 4 agent homes like that). That is
agent-launch's home-repair job (jokersquad), not pheobe's.
