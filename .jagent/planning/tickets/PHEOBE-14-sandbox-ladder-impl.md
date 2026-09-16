# PHEOBE-14 — sandboxing ladder implementation

| Field | Value |
|-------|-------|
| **ID** | PHEOBE-14 |
| **Priority** | P2 |
| **Status** | Backlog |
| **Assignee** | unassigned |
| **Dependencies** | none |
| **Estimated effort** | M |

## Problem

The sandbox ladder (DESIGN.md §"Sandboxing ladder") is design-only:
strict/moderate shell out to `buckets run`'s bwrap shape (argument order
proven by bro's `sandbox.rs`), free runs the guarded subprocess, and the
task schema lacks the `sandbox` field entirely.

## Success criteria

- [ ] `sandbox: "strict"|"moderate"|"free"` (default moderate) in the
      task schema + `PHEOBE_SANDBOX` env override (env wins).
- [ ] strict: bash tool executes via `buckets run -- bwrap` shape
      (mount+PID ns, network off, worktree rw + toolchain ro binds,
      matching bro's `BwrapLevel::Process` argument order); bash
      restricted to allowlisted command shapes (done_when, test/build
      runner prefixes).
- [ ] moderate: same binds with network on; free: guarded subprocess as
      today.
- [ ] Fail-closed: strict requested but no bwrap/buckets → intake error
      `blocked:"no_sandbox"` (never a silent downgrade); moderate without
      bwrap → process capsule + a doubt noting containment dropped.
- [ ] Invariants hold in every tier (no world writes, destructive scan
      always on, paths_allow, root jail) — one shared enforcement point,
      tested with a fake bwrap script proving the binds on the command
      line.
