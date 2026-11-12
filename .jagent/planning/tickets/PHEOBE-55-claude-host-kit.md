# PHEOBE-55 — Claude host-mode kit (`adopt/claude/pheobe-host.md`)

**Filed:** 2026-10-15, foreman (s481).
**Priority:** P3.

## Problem
Host mode has kits for opencode, cursor and agy, but not for Claude, which is the most-used host in the fleet. For BRO-135 (bro-cli PR #127) the foreman gave a Claude Code subagent the opencode host kit's protocol, plus hand-written notes:
- intake was already done by `host setup`;
- verify through `host finish`;
- never push;
- the trailer format.

It worked: 2 commits, 870/870 tests, scope clean. That's the evidence that the protocol carries over.

## Scope
- `adopt/claude/pheobe-host.md`: a Claude Code subagent definition (`.claude/agents/pheobe-host.md`) for host mode, with tools Bash, Read, Edit, Write, Grep and Glob, and `Task` disallowed. Two routes:
  - (a) the parent runs `pheobe host setup` and passes the worktree in;
  - (b) the subagent runs it itself.
  It ends on `pheobe host finish` and the JSON handoff. Include the trial's protocol notes as rules:
  - never commit `.pheobe/`;
  - run `done_when` through `host finish`, never by hand;
  - say in `doubts` which `paths_allow` areas `done_when` doesn't cover (until PHEOBE-54).
- The Agent SDK variant (`AgentDefinition`) of the same kit, as `sdk-snippet.md` has for self mode.
- `pheobe adopt claude-host` prints it; add it to the adopt README table; update the cli test listing adopt names.

## Done when
`pheobe adopt claude-host` prints the kit, the adopt README lists it, and the cli tests pass.
