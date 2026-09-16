---
slug: rust
name: Rust
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [rust, passport, tooling, cargo]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
  - cargo book — doc.rust-lang.org/cargo (workspaces, `[patch]`, features, `cargo package`)
  - clippy lints — rust-lang.github.io/rust-clippy/master/ (each lint's rationale + `#[allow]` name)
  - edition guide — doc.rust-lang.org/edition-guide (what `edition = "2024"` changes)
---

## What it is

The tooling passport for Rust: what a competent builder reaches for, and where the
ground truth about versions lives on disk. Rust's story is strong convention — cargo
owns almost every row of the table, which makes the wrong-answer risk concentrate in
version truth and workspace layout, not tool choice.

## Tooling surface

| passport row | value |
|---|---|
| build system | `cargo` (+ workspace for multi-crate repos) |
| package manager | `cargo` (crates.io; path-deps for sibling repos) |
| test runner | `cargo test` |
| format / lint | `rustfmt` / `clippy` |
| version truth | `Cargo.toml` + `Cargo.lock` **on disk** |

## Gotchas

- **Sibling path-deps must stay resolvable from a worktree.** Path deps like
  `../<name>` resolve relative to the *manifest*, so a worktree that isn't a peer of
  the siblings breaks them silently. When a workspace gets provisioned into a
  worktree (`buckets`/`kitchen` doctrine), check that every `../` path still points
  at the right tree before assuming a missing-crate error is a real bug.
- **Use the lockfile as version truth, never your memory of the ecosystem.** Before
  adding a dep or reporting a version, read the crate's version from `Cargo.lock`
  (and, when in doubt, the vendored/installed source — `cargo doc --open` or the
  `.cargo/registry/src` tree), not from recall. A model's idea of "current stable
  Rust" is wrong more often than it is right.
- **Never `rustup` to a newer toolchain mid-run.** If `rust-toolchain.toml` pins a
  channel, honor it; upgrading mid-task changes compiler errors and clippy lints out
  from under you and invalidates the diagnostics you were acting on.
- **Read the actual failure text before proposing a fix.** Cargo/rustc errors
  (especially `error[E0308]` mismatches and trait-bound cascades) name the specific
  mismatched types and the source span — the fix is usually in the message. Wall-of-
  errors output is mostly duplicates of the first root error; fix that one and
  recompile rather than addressing all 40 lines.
- **`cargo check` before `cargo build`.** It's the fast correctness signal; a full
  build (or worse, `--release`) is wasted minutes when the first error would have
  surfaced in seconds.
