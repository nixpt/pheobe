# `pheobe verify` prints only "verify failed" when done_when fails

**Found:** 2026-09-16, foreman s457 adoption test, host mode. The gate
exited 1 with nothing but `pheobe: verify failed`; the host (a model, by
design) had to re-run the done_when command by hand to learn that two
pre-existing tests were failing. `cmd_verify` prints allowlist violations
but nothing for a done_when failure — no command, no exit code, no output
excerpt, even though `run_done_when` already captures all three.
**Severity:** P3 — host mode's whole premise is a mechanical gate the host
reads; a bare "failed" makes the host guess.

**Status:** Done (PHEOBE-22)

## Expected behavior

```
✗ done_when failed: python3 test_calc.py (exit 1)
  FAIL test_div_true_division AssertionError()
  …
```

## Fix sketch

In `main.rs::cmd_verify`, when `!ev.passed`, print `ev.ran`, the exit code
and `ev.output_excerpt` to stderr before bailing.
