---
slug: c-cpp
name: C / C++
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [c, cpp, passport, tooling, cmake]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
  - CMake reference — cmake.org/cmake/help/latest (presets, targets, `CMAKE_EXPORT_COMPILE_COMMANDS`)
  - clang tooling — clang.llvm.org/docs (ClangFormat, clang-tidy checks, sanitizers)
  - vcpkg — vcpkg.io/en/docs · Conan — docs.conan.io (manifest-mode dependency truth)
---

## What it is

The tooling passport for C/C++: the ecosystem where the build system is always
project-specific and "does it build" is the hardest gate. Read the build files
before running anything.

## Tooling surface

| passport row | value |
|---|---|
| build system | `make` / `cmake` / `meson` — whichever the repo carries |
| package manager | `vcpkg` / `conan` (system packages are a fallback, not truth) |
| test runner | `ctest` / googletest |
| format / lint | `clang-format` / `clang-tidy` |
| version truth | `CMakeLists.txt` (+ `-std=` / `CMAKE_CXX_STANDARD`) |

## Gotchas

- **Never run a bare `gcc`/`clang++` line of your own invention.** The repo's build
  files encode includes, defines, standards, and optimization levels that a
  one-off invocation silently misses — and a "fix" validated that way isn't a fix.
  Configure and build through the project's own system.
- **Out-of-source builds only.** CMake/Meson artifacts (`CMakeCache.txt`,
  `build/`) must never land in the source tree; a stale in-source cache causes
  phantom errors that look like code bugs. On weird build failures, wipe the build
  dir and reconfigure before blaming the code.
- **Use the installed toolchain version, not the newest.** A repo pinned to an
  older standard (`C++17`, `-std=c99`) breaks under a newer compiler with
  different defaults. Don't `apt install` a different GCC mid-run to "see if it
  works" — that's two variables changed at once.
- **Header dependencies are invisible to incremental build systems.** If headers
  changed and the build "succeeds," suspect stale objects; clean rebuild before
  trusting green.
- **Undefined behavior compiles fine.** clang-tidy/`-fsanitize=address,undefined`
  (when the build supports them) are how C/C++ "works" gets verified — green
  compilation means nothing about memory safety.
