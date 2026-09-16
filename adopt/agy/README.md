# pheobe — Antigravity (AGY) adoption kit

Antigravity (`agy`) integrates with pheobe in two directions:
1. **Host adoption**: AGY agents dispatch tasks to pheobe via progressive-disclosure Skills or Subagents.
2. **Worker engine**: Pheobe drives `agy` CLI in headless print mode (`PHEOBE_PROVIDER=agy` or `antigravity`).

## 1. Host Adoption (AGY -> Pheobe)

### A. Skill Route (Recommended)

Copy `adopt/agy/skill.md` to:
- Workspace scope: `.agents/skills/pheobe/SKILL.md` (or `.agent/skills/pheobe/SKILL.md`) in your project root.
- User global scope: `~/.gemini/antigravity-cli/skills/pheobe/SKILL.md`.

Antigravity will progressively disclose the `pheobe` skill to the agent when the user or task asks to run a scoped coding task with a mechanical verification check.

### B. Subagent Route

In Antigravity conversations or IDE workflows, define a `pheobe` subagent equipped with command execution tools:

```json
{
  "name": "pheobe-dispatcher",
  "description": "Dispatches concrete coding tasks to pheobe worktrees.",
  "system_prompt": "You are a pheobe task runner. Given a task with a verification command, write a task JSON to /tmp/pheobe-task.json and run 'pheobe run /tmp/pheobe-task.json --json'. Return the parsed handoff report."
}
```

## 2. Worker Engine (Pheobe -> AGY)

Set `PHEOBE_PROVIDER=agy` (or `PHEOBE_PROVIDER=antigravity`).

Pheobe runs:
```bash
agy -p "<prompt>" --output-format json --dangerously-skip-permissions
```
with `cwd` set to the provisioned worktree.

### Environment & Tuning
- `PHEOBE_AGY_BIN`: path to `agy` executable (default: `agy` from PATH).
- `PHEOBE_AGY_FLAGS`: extra or replacement CLI flags (default: `--dangerously-skip-permissions`).
- `PHEOBE_AGY_TIMEOUT_SECS`: max turn runtime before killing (default: 3600).
- `PHEOBE_SANDBOX`: sandbox tier (`strict` adds `--sandbox`; `moderate` and `free` omit).

## 3. The Shared JSON Contract

Every run returns the standard handoff report:

```json
{
  "ok": true,
  "task": "<task description>",
  "branch": "pheobe/<slug>-<id>",
  "worktree": "<path to isolated worktree>",
  "commits": ["<sha message>"],
  "tests": { "ran": "<verification command>", "passed": true },
  "summary": "<one paragraph summary>",
  "next_steps": ["<actions for the parent>"],
  "doubts": ["<unverified assumptions>"],
  "blocked": "<verbatim blocker if ok is false>"
}
```
