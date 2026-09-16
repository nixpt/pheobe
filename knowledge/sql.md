---
slug: sql
name: SQL
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [sql, sqlite, passport, tooling, migrations]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
---

## What it is

The tooling passport for SQL: schema truth lives in migration files and schema
definitions on disk, dialect is repo-specific, and the query planner — not your
intuition — decides what a query costs.

## Tooling surface

| passport row | value |
|---|---|
| build system | none inherent — the schema/migration files are the artifact |
| package manager | migrations tooling (repo-specific: alembic/golang-migrate/sqlx/etc.) |
| test runner | fixture-based tests through the repo's own harness |
| format / lint | `sqlfluff` |
| version truth | the schema/migration files in the repo (e.g. `migrations/`, `schema.sql`) |

## Gotchas

- **Read the actual schema before writing a query.** Column names, nullability,
  defaults, and constraints come from the migration/schema files in the repo —
  never from memory of "what such a table usually looks like." Guessing a column
  name is a guaranteed failure mode.
- **Match the dialect.** SQLite, Postgres, and MySQL differ on `RETURNING`,
  `INSERT ... ON CONFLICT/ON DUPLICATE KEY`, `ILIKE`, date functions, and type
  names. The repo's engine decides; a query valid in one dialect is a syntax or
  semantic error in another.
- **Never mutate schema or data without the migrations tool.** Hand-edited
  databases (or hand-issued `ALTER TABLE` against the live schema) desync from the
  migration history and break every future migration run. New schema change = new
  migration file.
- **Test migrations on a fixture, not production.** Fixture-based testing exists
  because migrations are irreversible-in-effect: run them against a copy, verify
  the resulting schema, then commit the migration file.
- **Trust `EXPLAIN`, not intuition.** Query performance claims (index usage, join
  order, row estimates) must come from `EXPLAIN [QUERY PLAN]` on the real engine —
  a model's per-row cost intuition is routinely wrong by orders of magnitude.
