---
slug: go
name: Go
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [go, golang, passport, tooling]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
---

## What it is

The tooling passport for Go: the most uniform of the eight languages — one toolchain
owns build, test, format, and vet — so the wrong-answer risk concentrates in the
module graph and version truth.

## Tooling surface

| passport row | value |
|---|---|
| build system | the `go` toolchain itself (`go build`) |
| package manager | `go mod` (modules; no separate tool) |
| test runner | `go test` |
| format / lint | `gofmt` / `go vet` |
| version truth | `go.mod` (and `go.sum` for hashes) on disk |

## Gotchas

- **`go.mod` is the single source of truth — including the toolchain.** The `go`
  directive pins the language version and, since 1.21, the toolchain via
  `toolchain`/`GOTOOLCHAIN`. Do not "helpfully" upgrade either mid-run; a newer
  language version can change loop-variable capture semantics under existing code.
- **`gofmt` output is not a style choice — it is the only accepted
  style.** Formatting is settled; never hand-tune whitespace, and don't fight the
  formatter in review.
- **`go test -run <pattern>` first.** Full-suite runs are slow in large modules;
  run the specific failing test (plus `-count=1` to defeat result caching when
  iterating) before concluding anything about the suite.
- **Missing module errors: run `go mod tidy`, don't hand-edit `go.mod`.** Hand-edits
  desync `go.mod` from `go.sum` and produce checksum errors on the next clean
  checkout.
- **`go vet` findings are real.** Unlike some linters, vet's default checks (printf
  misuse, lock copying, struct tags) catch genuine bugs; treat its output as a
  failing gate, not advice.
