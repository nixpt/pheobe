---
slug: shell
name: Shell
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [shell, bash, passport, tooling, shellcheck, bats]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
  - ShellCheck wiki — shellcheck.net/wiki (every SC code, with the fix)
  - bats-core — bats-core.readthedocs.io (test layout, `setup`/`teardown`, `run`)
  - shfmt — github.com/mvdan/sh (formatter; `-i 4 -ci` conventions) · POSIX sh — pubs.opengroup.org/onlinepubs/9699919799/utilities/V3_chap02.html
---

## What it is

The tooling passport for shell: the highest-gotcha-per-line language in the set.
There is no package manager and no manifest; the shebang is the version truth, and
lint-before-run is the discipline.

## Tooling surface

| passport row | value |
|---|---|
| build system | none — lint-managed (`shellcheck` as the gate) |
| package manager | none (system PATH; manage tool availability, not versions) |
| test runner | `bats` (bash automated testing system) |
| format / lint | `shfmt` / `shellcheck` |
| version truth | the shebang line (`#!/usr/bin/env bash` etc.) |

## Gotchas

- **`shellcheck` before every run.** Most shell bugs (unquoted `"$var"`, missing
  `set -e`/-`u`, word-splitting on spaces) are exactly what shellcheck flags;
  fixing them costs seconds, debugging them at runtime costs the task.
- **Quote every expansion: `"$var"`, not `$var`.** The single most common bug class.
  Word-splitting and globbing on unquoted expansions fail only on inputs with
  spaces/globs — which are exactly the inputs a test run doesn't exercise.
- **`set -euo pipefail` at the top of every non-trivial script.** Without it, a
  failing command mid-script is silently ignored and the script's exit code
  reflects only the last line. Check exit codes explicitly when you *want* a
  non-fatal failure.
- **Bash ≠ sh.** A `#!/bin/sh` script that uses bash-isms (arrays, `[[ ]]`,
  `local` with assignment) breaks on the system sh (often dash). Make the shebang
  and the syntax agree.
- **`[[ ]]` is bash-only; avoid test gotchas.** Inside `[ ]`, unquoted empty
  variables become syntax errors (`[ = foo ]`); `==`/`<` in `[ ]` are non-portable.
  Prefer `[[ ]]` under a bash shebang, and always quote on both sides.
