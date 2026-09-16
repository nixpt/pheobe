---
slug: java-kotlin
name: Java / Kotlin
kind: external
version: "passport-1"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: false
tags: [java, kotlin, passport, tooling, gradle, maven]
sources:
  - DESIGN.md §"Language & framework competence" (pheobe repo — the passport table)
---

## What it is

The tooling passport for the JVM languages: Gradle and Maven own the build/package/
test rows, and both are slow enough that the main discipline is "use the wrapper,
run the narrowest task."

## Tooling surface

| passport row | value |
|---|---|
| build system | `gradle` / `maven` — always via the wrapper (`./gradlew`, `./mvnw`) |
| package manager | same tool (Maven Central / repositories declared in the build files) |
| test runner | JUnit (via the build tool) |
| format / lint | `ktlint` / spotless (Gradle plugin) |
| version truth | `build.gradle*` / `pom.xml` (and the lockfile if `gradle lock` is used) |

## Gotchas

- **Always use the wrapper (`./gradlew` / `./mvnw`), never a system-installed tool.**
  The wrapper pins the build-tool version the repo was written against; a different
  Gradle/Maven can change plugin resolution and break the build in ways that look
  like code errors.
- **Use the pinned JDK.** JVM language level (`sourceCompatibility`,
  `<release>`, Kotlin `jvmToolchain`) is version truth; don't point the build at a
  newer JDK to "get it to run" — module-system and core-library changes can
  silently alter behavior.
- **Run the narrowest task.** `./gradlew :module:test --tests "FooTest.bar"` and
  `./mvnw -pl <module> test -Dtest=FooTest#bar`, not the full suite — JVM builds
  and tests are slow, and a full-suite run mid-iteration wastes the clock.
- **Gradle daemon state can lie after build-file edits.** If you just changed
  `build.gradle*` and get inexplicable results, re-run with `--no-daemon` once
  before diagnosing the code.
- **Dependency conflicts are silent at compile time.** Version clashes are resolved
  (Maven: nearest-wins; Gradle: highest-wins) without errors and bite at runtime.
  For dependency behavior questions, read the resolved jar's classes (or its
  sources jar), not training data — the *resolved* version may differ from the
  declared one.
