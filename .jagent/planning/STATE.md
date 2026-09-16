# Planning state — pheobe

**Updated:** 2026-09-16
**Milestone focus:** M0 — the loop exists; exit gate must pass a real run

**Branch:** `main` — PHEOBE-1 (model turn), PHEOBE-2 (aging ladder), PHEOBE-3
(all three field defects fixed, issues 01-03 closed) merged; designs for
sandbox ladder + loops/graphs + host integrations (opencode/claude/codex/
cursor/kimi/ai-sdk) landed. Verified live 2026-09-16 twice: foreman s456 (Zen, 3 models — exit-gate
defects found), then W0 dogfood (PHEOBE-13) on fixed main: deepseek-v4-pro
(5 turns), kimi-k2.6 (8), glm-5.2 (6) — all `ok:true`, one `calc.py`-only
commit each with Pheobe-Task trailer, aging hard-stop and vague-ask
rejection verified live. Issue 04 filed (failed model run leaves empty
worktree).

## Wave plan (sub-agent-ready batches)

| wave | tickets | runs-in-parallel? | notes |
|---|---|---|---|
| W0 | PHEOBE-13 (dogfood Zen, P0) | solo | field-verify the fixed exit gate before anything else |
| W1 | PHEOBE-9 (Worker trait) + PHEOBE-10 (barn hardening) + PHEOBE-12 (knowledge seed) + PHEOBE-15 (adopt kits) | DONE 2026-09-16 — 4 parallel sub-agents, 4 worktrees, merged sequentially (34 tests green on main) | main.rs/tests.rs conflicts resolved by hand (kept both test blocks) |
| W2 | PHEOBE-4, 5, 7 (opencode, claude, codex adapters) | DONE 2026-09-16 — 3 parallel sub-agents; all three live-smoked green (opencode `step_finish` tokens, claude full-run ok:true, codex exec CLI path) | 60 tests on main; CLI-first decision for codex recorded; wait-timeout dep added by PHEOBE-5 |
| W3 | PHEOBE-14 (sandbox impl) + PHEOBE-11 (structural ladder) + PHEOBE-6 (cursor adapter) | yes | independent surfaces |
| W4 | PHEOBE-8 (kimi) + PHEOBE-16 (memory swap) + PHEOBE-17 (acp) | mostly | 16 touches learn.rs which 8 may read |
| W5 | PHEOBE-18 (release posture) | solo | after dogfood + quartet |

RULES.md applies recursively: new defects found during any wave get their
own issue; tickets' `Status` flips with a `## Resolution` section.
