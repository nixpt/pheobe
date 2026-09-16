---
slug: crush
name: Crush language (crush-ast / CAST / CASM / crush-vm)
kind: external
version: "crush-ast 0.3.7 (43 crates)"
last_verified: 2026-09-16
verified_by: nixp
cutoff_gap: true
tags: [crush, crush-ast, cast, casm, crush-vm, fleet-language, polyglot, exosphere]
sources:
  - canonical workspace: /workspace/projects/crush-ast (Cargo.toml v0.3.7 + crates/ listing, read 2026-09-16)
  - lexer ground truth: /workspace/projects/crush-ast/crates/crush-frontend/src/parser/lexer.rs (keyword match, read 2026-09-16)
  - /workspace/projects/exosphere/.jagent/skills/crush-lang/SKILL.md (syntax card + CASM categories; 2026-07-13, partially stale — see Gotchas)
  - /workspace/projects/exosphere/.jagent/knowledge/architecture/crush_ast_peer_architecture.md (the peer split + walker design)
---

## What it is

Crush is the fleet's own capability-based polyglot scripting language: source →
parser → **CAST** (typed AST, `crush-cast`) → compiler → **CASM** (bytecode,
`casm`) → **crush-vm** (CVM1 / FastVM / crush-jit). Post-cutoff: it is in NO
training data. Two implementations exist — do not conflate them:

| implementation | where | VM crate | status |
|---|---|---|---|
| **crush-ast** (standalone) | `projects/crush-ast/`, **v0.3.7, 43 crates** | `crush-vm` | **canonical source of truth** |
| exosphere in-tree | `projects/exosphere/crates/core/` | `nanovm` (pre-rename, NOT migrated yet) | older twin; divergence tracked in exosphere `.jagent/planning/TASKS.md` |

Both lower to the same CASM. When writing Crush, the crush-ast lexer/compiler
is ground truth — the exosphere skill doc (2026-07-13) predates the 0.3.x growth
(says 35 crates / v0.2.0; misses the `new` keyword and the multilingual aliases).

Fleet ecosystem, so a task lands in the right repo: `crush-ast` (language + VM +
walkers `crush-lang-{bash,c,custom,dart,go,java,js,nepali,python,rust,wasm,zig,zsh}`
+ `tree-sitter-crush`, `crush-lint`, `crush-pkg`, `crush-debugger`, `crush-aot`,
`crush-jit`, `crush-ffi`) · `crush-workspace/` (crush-language-guide mdBook,
crush-lsp, crush-notebook, polydex, crush-visuals, crush-vscode) ·
`crush-capsules/` (CRUSH capsule ecosystem) · exosphere (Crush as conductor +
agent front-end; CrushVM sandbox gap tracked in v1.0 planning) · polydex indexes
`.crush` via tree-sitter-crush.

## Language surface (verified against the canonical lexer 2026-09-16)

Keywords: `let mut fn if else while for in return try catch throw break
continue struct use capability async await spawn yield new export lang import
match true false null` — plus multilingual aliases (`karya`/`函数`/`関数` = fn,
`yadi` = if, `manau` = let, `sahi`/`galat` = true/false, … Nepali/Chinese/Japanese).

```crush
let x = 42;                       // dynamically typed; hints annotation-only
let name: String = "hello";
fn add(a: Int, b: Int) -> Int { return a + b; }
let double = |x| { return x * 2; };   // lambdas: |params|, NOT fn(params)
let square = |x| => x * x;           // arrow form, single expression
let r = data |> process |> format;   // pipeline, lowest precedence
match v { 0 => print("zero"), _ => print("other") }   // implemented
struct Point { x: Float, y: Float }                    // implemented
try { throw "Oops"; } catch e { print(e); }            // implemented
@io.print("hello"); @fs.read("f.txt");                // capability calls
@python { import math; print(math.pi) }               // polyglot blocks
use @mcp "https://…" { "issues.list" } as github;      // @mcp/@cap/@lang/@git/@http/@file imports
spawn worker(); yield; async fn f() { await g(); }
```

**Agents skip the parser**: emit CAST JSON directly → `crush_cast::validate_json`
(`crush-cast/src/validate.rs`) → `crush_frontend::compile_cast(&Program)` →
CASM → crush-vm. `Program.cast_version` is required. AI-native node types:
`Query`, `ToolChain`, `AgentDelegation`, `LearningLoop`, `ContextAware` /
`GoalDeclaration`, `ProgressUpdate`, `KnowledgeSharing`, `CapabilityDiscovery`.

## Gotchas

- **`spawn` is a keyword** (actor concurrency) — name process-spawning helpers
  `run_tool`/`launch`, never `spawn`.
- **Statements are newline-terminated**: no multi-line calls, no list literals
  split across lines.
- **No `finally`, no `??`, `as` is NOT a keyword** (verified: absent from the
  lexer keyword match) — sequential code after `catch`; explicit null checks.
- `let mut` ≡ `let` at runtime today (`mut` is reader-signal only); type hints
  do not enforce.
- The Bash polyglot walker is minimal (`NAME=VALUE` + `echo` patterns);
  Python/Rust walkers are partial; JS is the most complete. Complex foreign
  code silently does nothing — verify a walker covers it before relying on
  `@lang` blocks.
- exosphere's in-tree docs may cite `nanovm` — same VM family, older name.
  `cargo test -p crush-frontend -p crush-cast -p crush-vm` (in crush-ast) is
  the health check; full guide: `projects/crush-workspace/crush-language-guide/`.
