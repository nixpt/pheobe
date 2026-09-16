# RULES — standing discipline for anyone working this backlog

Standing developer discipline for anyone working this backlog. These are not suggestions — every
agent touching `.jagent/planning/`'s backlog follows them.

## 1. Verify before you fix

**A ticket's `Backlog` status is a claim, not a fact.** Before spending any effort on a ticket:

1. Re-run its own `## Reproduction`/`## Success criteria` section verbatim, against current
   `main` (pull first, run against unmodified main).
2. If it no longer reproduces / is already met: update the ticket's `Status` to `Done` with a
   one-paragraph `## Resolution` section and mark the corresponding `TASKS.md` line `[x]`. Do
   not silently delete the ticket.
3. If it does reproduce: proceed to fix it.

This applies recursively: if you find a NEW bug while working an existing ticket, don't fold it
silently into the same commit — file it as its own ticket so it gets its own verify-before-fix
cycle later.

## 2. One worktree + branch per milestone (or per ticket, whichever is smaller)

**Every unit of work gets its own worktree and its own branch.** Never work directly on `main`.

## 3. Commit + push at every milestone/phase boundary — don't batch to the end

Push when a milestone (or ticket) is done — not when the whole backlog is done. Before starting
the next milestone, pull the latest `main` into a fresh worktree+branch.

## 4. Update `.jagent/planning/` as you go, not as an afterthought

- Mark `TASKS.md` checkboxes `[x]` the moment something is verifiably done.
- Update the closed ticket's own `Status` field and add a `## Resolution` section.
- New ticket found while working something else → use `templates/ticket.md`, next ID.

## 5. Never a box-local absolute path-dep

If this project needs to depend on another sibling repo, use a git dependency
(`{ git = "https://github.com/<org>/<repo>", rev = "<sha>" }`), never a `path = "/home/..."`
or a relative `../../..` path that assumes a specific machine's directory layout. See
`workspace-meta/FOREMAN_THREADS.md`'s "~17 box-local path-deps" entry for exactly why this
matters — it is the single largest source of "works on my box, dies in CI" in this fleet.
