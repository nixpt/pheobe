---
slug: python
name: Python
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [python, passport, tooling, pytest]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
---

## What it is

The tooling passport for Python: a language whose build/package story is the least
standardized of the eight — there is no single default, and the *repo on disk* tells
you which one applies. Never assume `pip`; read the manifest.

## Tooling surface

| passport row | value |
|---|---|
| build system | none inherent — `uv`, `poetry`, `pip`, `hatch`, `setuptools` per the manifest |
| package manager | `uv` / `pip` (read `pyproject.toml` to pick) |
| test runner | `pytest` |
| format / lint | `ruff` (lint+format) / `black` |
| version truth | `pyproject.toml`, `uv.lock` (or the project's own lockfile) on disk |

## Gotchas

- **Python version ≠ your memory.** The interpreter that matters is whatever
  `.python-version`, `pyproject.toml` (`requires-python`), or the venv on disk says —
  never the one a model recalls as current. A 3.12-only feature invoked under a
  3.10 interpreter is a self-inflicted bug.
- **Read the manifest to pick the workflow — `pip install` is the wrong default.**
  If `uv.lock` exists use `uv sync`/`uv run`; if `poetry.lock` exists use `poetry`;
  only fall back to `pip` for bare `requirements.txt` repos. Mixing managers into one
  tree (e.g. `pip install` inside a uv project) corrupts the lockfile story.
- **Use the venv, don't create a second one.** Every repo that has one already has a
  `.venv` (or the manager's own env) — install and run *inside it* rather than
  installing into a system or global Python, which can break the host's tooling.
- **`pytest` failure output is the spec, not the summary.** The traceback shows the
  exact assertion with the diff of left/right values; `assert result == 3` with a
  `Falsifying example` line (hypothesis) means a property broke — read that example,
  it is a ready-made regression case.
- **Don't guess an API into existence.** For any library call you're not certain
  about, read the installed source in the venv's `site-packages` before writing the
  call — Python's dynamic surface makes wrong-signature bugs fail late, at runtime,
  sometimes silently.
