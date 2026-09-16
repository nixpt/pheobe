# PHEOBE-13 — live dogfood: rerun the Zen scenario on fixed main

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-13 |
| **Priority** | P0 |
| **Status** | Backlog |
| **Assignee** | unassigned |
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
