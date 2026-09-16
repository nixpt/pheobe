# PHEOBE-13 — live dogfood: rerun the Zen scenario on fixed main

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-13 |
| **Priority** | P0 |
| **Status** | Done |
| **Assignee** | nixp (this session) |
| **Dependencies** | none (PHEOBE-3 fixes are on main) |
| **Estimated effort** | S |

## Problem

The first live Zen run (foreman s456) proved the model turn + tool loop
correct on `deepseek-v4-pro`, `kimi-k2.6`, `glm-5.2` — and blocked every
run at the exit gate on the three .jagent issues, all now fixed on main
(PHEOBE-3). The exact scenario must be re-verified end-to-end on the
fixed build before any further adapter work, per RULES.md's
verify-before-fix spirit (this time forward: verify the fix against the
field, not just the unit test).

## Success criteria

- [ ] Scratch git repo (calc.py/check.py pattern) + task JSON with
      `paths_allow: ["calc.py"]`, run with
      `PHEOBE_BASE_URL=https://opencode.ai/zen/v1` against all three
      models.
- [ ] Every run: report `ok:true`, `tests.passed:true`, exactly one
      commit with the `Pheobe-Task:` trailer, `.pheobe/` and run
      byproducts absent from the commit, no allowlist violation.
- [ ] At least one run with `ttl` set small enough to observe the aging
      warn inject in stderr logs (proves injects fire live, not only in
      unit tests).
- [ ] One deliberately vague task still rejected at intake.
- [ ] Results recorded in `.jagent/planning/STATE.md` (verified-live
      line updated) and any new defect filed as its own issue per
      RULES.md.

## Resolution

W0 dogfood, 2026-09-16, on fixed main (`ca2a969` build). Same scratch
scenario as foreman s456 (calc.py subtract bug, check.py, paths_allow
`["calc.py"]`, ttl 10m, max_iterations 4), endpoint
`https://opencode.ai/zen/v1`, key via secure-env (`OPEN_CODE_ZEN_GO`).

All criteria green:

- **deepseek-v4-pro**: `ok:true`, 5 turns, commit `6cb7ff9` —
  `calc.py | 2 +-` only, `Pheobe-Task:` trailer, tests.passed ("add ok").
- **kimi-k2.6**: `ok:true`, 8 turns, commit `603880c` — same clean diff.
- **glm-5.2**: `ok:true`, 6 turns, commit `fdac622` — same clean diff.
- **Aging observed live**: `ttl:"0s"` run → hard stop,
  `blocked: "ttl_exceeded — task-design failure, not a time problem"`.
- **Vague ask rejected at intake** (live): "Polish the API and also
  improve error handling" → exit 1, fuzziness-gate message.

All three models produced byte-identical diffs (`a - b` → `a + b`),
consistent with the s456 observation that the loop, not the model, is
the contract. New hygiene defect filed as issue 04 (failed model run
leaves empty worktree). Ticket closed; wave W0 complete.
