---
slug: typescript-javascript
name: TypeScript / JavaScript
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [typescript, javascript, node, passport, tooling, npm]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
  - Node.js docs — nodejs.org/docs (ESM vs CJS resolution, `package.json` `exports`)
  - pnpm — pnpm.io · npm — docs.npmjs.com (lockfile names decide the package manager)
  - TypeScript handbook + tsconfig reference — typescriptlang.org/tsconfig · vitest — vitest.dev · eslint — eslint.org
---

## What it is

The tooling passport for the TS/JS ecosystem, where every row of the table has
competing answers (npm vs pnpm vs bun; vitest vs jest; biome vs prettier+eslint).
The repo on disk — and only the repo on disk — decides which set applies.

## Tooling surface

| passport row | value |
|---|---|
| build system | `tsc` for types / the bundler for artifacts (`vite` et al.) |
| package manager | `npm` / `pnpm` / `bun` — whichever the lockfile indicates |
| test runner | `vitest` / `jest` (read `package.json`) |
| format / lint | `biome` / `prettier` / `eslint` |
| version truth | `package.json` + **the lockfile** on disk |

## Gotchas

- **The lockfile names the package manager.** `package-lock.json` → npm,
  `pnpm-lock.yaml` → pnpm, `bun.lockb` → bun. Using a different manager than the
  lockfile was generated with produces a second, conflicting dependency tree —
  sometimes silently, always wrongly.
- **Never update deps mid-run.** `npm install <pkg>@latest` / `npx` pulling the
  newest tool version mid-task changes behavior underneath you. Run the repo's
  pinned versions (check the `packageManager` field if present) and use `npm ci` /
  the manager's lockfile-respecting install, not a bare install that can resolve
  fresh versions.
- **`tsc` type errors are not runtime errors.** Don't "fix" a type error by casting
  (`as any`) to make it go away; it converts a compile-time diagnosis into a
  runtime bomb. Read the error, fix the type.
- **node_modules is ground truth for API shape.** For any framework call, read the
  `.d.ts` in `node_modules` — training-data APIs for fast-moving libraries (React,
  vite, test runners) are frequently one major version stale.
- **Run scripts via the manifest (`npm run` / `pnpm test`), not bare commands.**
  Repos pin versions, flags, and env in `package.json` scripts; invoking the tool
  directly bypasses them and can behave differently.
