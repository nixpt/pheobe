# .jagent/planning — pheobe

Execution board for pheobe.

## Directory map

```
planning/
├── README.md           # this file
├── STATE.md            # current project state (per-session updates)
├── ROADMAP.md          # milestones, non-goals (mirrors DESIGN.md)
├── TASKS.md            # kanban: P0-P3
├── RULES.md            # standing discipline (verify-before-fix, worktree per ticket)
├── templates/          # ticket.md, issue.md
└── tickets/            # PHEOBE-N.md — one file per planned unit of work
../issues/              # NN-slug.md — defects observed in real runs (repro + expected + actual)
```

Tickets are *planned work*; issues are *observed defects*. An issue graduates to a
ticket when someone commits to fixing it (link both ways).
