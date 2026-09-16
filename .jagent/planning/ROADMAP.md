# ROADMAP — pheobe

Source of truth for scope is `DESIGN.md`. This file only tracks sequencing.

## M0 — the loop exists (in progress)

- PHEOBE-1 ✅ the model turn — OpenAI-shaped client + tool-calls loop over the barn
- PHEOBE-2 ⏳ aging ladder + budget enforcement in the loop (`agent/nixp/PHEOBE-2`)
- Exit gate correctness: `done_when` + `paths_allow` must pass a real run end-to-end
  (see `.jagent/issues/01`, `02` — today every successful model turn is rejected by the gate)

## M1 — adoptable

- Adoption kits (`adopt/{claude,codex,opencode}`) verified against a real harness each
- Host mode: `pheobe verify` as the exit gate for a parent's own model
- Provider decoupling review (design commit 68cec3e)

## M2 — learn

- Closed-loop learning store (sessions/events/nudges) graduated from opt-in

## Non-goals (from DESIGN.md)

No TUI, no daemon, no HTTP server, no session store; never edits the parent's checkout.
