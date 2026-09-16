//! Structured test-output parsing — bro's `tools/test.rs` pattern (BRO-93
//! lineage) ported to pheobe's std-only shape: hand-rolled line scanners
//! instead of regex, so the dep set stays small. Runners: cargo test, jest,
//! pytest, go test. The raw excerpt always stays alongside; this only adds
//! the machine-checkable shape.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestReport {
    pub runner: String,
    pub total: u64,
    pub passed: u64,
    pub failed: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<Failure>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Failure {
    pub file: String,
    pub name: String,
    pub text: String,
}

/// Parse combined test-runner output into structured form. Order matters:
/// cargo's summary line is checked before pytest's token scan, jest's
/// `Tests:` line before both, go last (its markers are the loosest).
pub fn parse(output: &str) -> Option<TestReport> {
    parse_cargo(output)
        .or_else(|| parse_jest(output))
        .or_else(|| parse_pytest(output))
        .or_else(|| parse_go(output))
}

/// Number immediately preceding `word` in `line` ("42 passed" → 42).
fn count_in(line: &str, word: &str) -> Option<u64> {
    let toks: Vec<&str> = line.split_whitespace().collect();
    for (i, t) in toks.iter().enumerate() {
        let cleaned = t.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_');
        if i > 0 && cleaned.eq_ignore_ascii_case(word) {
            let num = toks[i - 1].trim_matches(|c: char| !c.is_ascii_digit());
            if let Ok(n) = num.parse::<u64>() {
                return Some(n);
            }
        }
    }
    None
}

fn push_failure_dedupe(failures: &mut Vec<Failure>, file: &str, name: &str, text: String) {
    let name = name.trim().to_string();
    if name.is_empty() || failures.iter().any(|f| f.name == name) {
        return;
    }
    failures.push(Failure {
        file: file.to_string(),
        name,
        text,
    });
}

// ── cargo test ───────────────────────────────────────────────────────────────

fn parse_cargo(text: &str) -> Option<TestReport> {
    // "test result: ok. 42 passed; 0 failed; 0 ignored; ..."
    // "test result: FAILED. 10 passed; 1 failed; ..."
    let line = text.lines().find(|l| l.contains("test result:"))?;
    let passed = count_in(line, "passed")?;
    let failed = count_in(line, "failed").unwrap_or(0);
    let total = passed + failed;

    let mut failures: Vec<Failure> = Vec::new();
    // run lines: "test tools::write_bad ... FAILED"
    for l in text.lines() {
        let t = l.trim();
        if let Some(rest) = t.strip_prefix("test ") {
            if let Some(name) = rest.strip_suffix("FAILED") {
                let name = name.trim().trim_end_matches("...").trim();
                push_failure_dedupe(&mut failures, "", name, format!("test failed: {name}"));
            }
        }
    }
    // failure-list section: bare indented names after a "failures:" header
    let mut in_section = false;
    for l in text.lines() {
        let t = l.trim();
        if t == "failures:" {
            in_section = true;
            continue;
        }
        if in_section {
            if let Some(name) = l.strip_prefix("    ") {
                let name = name.trim();
                if !name.is_empty()
                    && name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | ':' | '.'))
                {
                    push_failure_dedupe(&mut failures, "", name, format!("test failed: {name}"));
                }
            } else if !t.is_empty() {
                in_section = false;
            }
        }
    }
    // panic file refs: "thread 'x' panicked at src/foo.rs:42:" — assign to
    // failures still missing a file, in order
    for l in text.lines() {
        let Some(idx) = l.find("panicked at ") else {
            continue;
        };
        let rest = &l[idx + "panicked at ".len()..];
        let Some((f, _)) = rest.split_once(':') else {
            continue;
        };
        if !f.ends_with(".rs") {
            continue;
        }
        if let Some(fail) = failures.iter_mut().find(|f| f.file.is_empty()) {
            fail.file = f.to_string();
        } else {
            failures.push(Failure {
                file: f.to_string(),
                name: "panic".into(),
                text: "test panicked".into(),
            });
        }
    }

    Some(TestReport {
        runner: "cargo".into(),
        total,
        passed,
        failed,
        failures,
    })
}

// ── jest / npm ───────────────────────────────────────────────────────────────

fn parse_jest(text: &str) -> Option<TestReport> {
    // "Tests:       2 failed, 14 passed, 16 total"
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("Tests:"))?;
    let passed = count_in(line, "passed").unwrap_or(0);
    let failed = count_in(line, "failed").unwrap_or(0);
    let total = count_in(line, "total").unwrap_or(passed + failed);

    let mut failures: Vec<Failure> = Vec::new();
    for l in text.lines() {
        if let Some(name) = l.trim_start().strip_prefix('●') {
            let name = name.trim();
            if !name.is_empty() {
                push_failure_dedupe(&mut failures, "", name, format!("test failed: {name}"));
            }
        }
    }
    Some(TestReport {
        runner: "jest".into(),
        total,
        passed,
        failed,
        failures,
    })
}

// ── pytest ───────────────────────────────────────────────────────────────────

fn parse_pytest(text: &str) -> Option<TestReport> {
    // Real pytest summaries are ORDER-DEPENDENT on outcome, not format:
    //   "2 failed, 1 passed in 0.12s" / "5 passed in 1.20s"
    // so each count is scanned independently; the summary line is the last
    // one carrying one of the count words plus the trailing " in X.Ys".
    let line = text.lines().rev().find(|l| {
        l.contains(" in ")
            && ["passed", "failed", "error", "skipped"]
                .iter()
                .any(|w| count_in(l, w).is_some())
    })?;
    let passed = count_in(line, "passed").unwrap_or(0);
    let failed = count_in(line, "failed").unwrap_or(0);
    let errors = count_in(line, "error").unwrap_or(0);
    let skipped = count_in(line, "skipped").unwrap_or(0);
    let total = passed + failed + errors + skipped;

    let mut failures: Vec<Failure> = Vec::new();
    for l in text.lines() {
        let Some(rest) = l.trim_start().strip_prefix("FAILED ") else {
            continue;
        };
        let (ref_, _msg) = match rest.split_once(" - ") {
            Some((r, m)) => (r, m),
            None => (rest.trim(), ""),
        };
        let (file, name) = match ref_.split_once("::") {
            Some((f, n)) => (f.to_string(), n.to_string()),
            None => (String::new(), ref_.trim().to_string()),
        };
        push_failure_dedupe(&mut failures, &file, &name, format!("test failed: {ref_}"));
    }
    Some(TestReport {
        runner: "pytest".into(),
        total,
        passed,
        failed,
        failures,
    })
}

// ── go test ──────────────────────────────────────────────────────────────────

/// "    calc_test.go:12: expected 3" → "calc_test.go:12"
fn go_file_ref(line: &str) -> Option<String> {
    let idx = line.find("_test.go:")?;
    let start = line[..idx]
        .rfind(char::is_whitespace)
        .map(|p| p + 1)
        .unwrap_or(0);
    let rest = &line[start..];
    let mut it = rest.split(':');
    let f = it.next()?;
    let ln = it.next()?;
    ln.parse::<u64>().ok()?;
    Some(format!("{f}:{ln}"))
}

fn parse_go(text: &str) -> Option<TestReport> {
    let lines: Vec<&str> = text.lines().collect();
    let mut passed = 0u64;
    let mut failed = 0u64;
    let mut failures: Vec<Failure> = Vec::new();
    let mut saw_marker = false;
    let mut last_marker = 0usize;

    for (i, l) in lines.iter().enumerate() {
        let t = l.trim_start();
        if t.starts_with("--- PASS: ") {
            saw_marker = true;
            passed += 1;
            last_marker = i;
        } else if let Some(rest) = t.strip_prefix("--- FAIL: ") {
            saw_marker = true;
            failed += 1;
            let name = rest.split_whitespace().next().unwrap_or("").to_string();
            // log lines (with the file ref) precede the FAIL marker — search
            // back to the previous marker
            let file = lines[last_marker..i]
                .iter()
                .rev()
                .find_map(|prev| go_file_ref(prev))
                .unwrap_or_default();
            push_failure_dedupe(&mut failures, &file, &name, format!("--- FAIL: {name}"));
            last_marker = i;
        }
    }

    // go package-level lines look like `ok  \tpkg\t0.5s` / `ok  \tpkg\t(cached)`;
    // a bare `ok <name>` is some other runner's line protocol (a stdlib
    // python harness, a shell script) — still countable, not go.
    let mut go_shaped = false;
    if !saw_marker {
        // no per-test markers (e.g. non-verbose): package-level lines only
        for l in &lines {
            let t = l.trim_start();
            if t.starts_with("FAIL") {
                failed += 1;
                push_failure_dedupe(
                    &mut failures,
                    "",
                    t.trim(),
                    format!("package failed: {}", t.trim()),
                );
            } else if t.starts_with("ok ") || t.starts_with("ok\t") {
                passed += 1;
                let tail = t.trim_end();
                if t.starts_with("ok\t")
                    || t.starts_with("ok  ")
                    || tail.ends_with("(cached)")
                    || tail.ends_with('s')
                        && tail
                            .rsplit(['\t', ' '])
                            .next()
                            .is_some_and(|d| d.trim_end_matches('s').parse::<f64>().is_ok())
                {
                    go_shaped = true;
                }
            }
        }
        if passed + failed == 0 {
            return None;
        }
    }

    Some(TestReport {
        runner: if saw_marker || go_shaped {
            "go"
        } else {
            "ok-lines"
        }
        .into(),
        total: passed + failed,
        passed,
        failed,
        failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cargo_test_ok() {
        let text = "test result: ok. 42 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out";
        let r = parse(text).unwrap();
        assert_eq!(r.runner, "cargo");
        assert_eq!(r.total, 42);
        assert_eq!(r.passed, 42);
        assert_eq!(r.failed, 0);
        assert!(r.failures.is_empty());
    }

    #[test]
    fn parse_cargo_test_failed_with_panic_file() {
        let text = "running 11 tests\ntest tools::write_ok ... ok\ntest tools::write_bad ... FAILED\n\nfailures:\n\n---- tools::write_bad stdout ----\nthread 'tools::write_bad' panicked at src/tools.rs:42:5:\nassertion failed\n\nfailures:\n    tools::write_bad\n\ntest result: FAILED. 10 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out\n";
        let r = parse(text).unwrap();
        assert_eq!(r.passed, 10);
        assert_eq!(r.failed, 1);
        assert_eq!(r.total, 11);
        assert!(r
            .failures
            .iter()
            .any(|f| f.name.contains("tools::write_bad")));
        assert!(r.failures.iter().any(|f| f.file.contains("src/tools.rs")));
    }

    #[test]
    fn parse_jest_output() {
        let text = "FAIL src/b.test.ts\n\n● suite › does the thing\n\n  expect(1).toBe(2)\n\nTest Suites: 1 failed, 1 passed, 2 total\nTests:       2 failed, 14 passed, 16 total\n";
        let r = parse(text).unwrap();
        assert_eq!(r.runner, "jest");
        assert_eq!(r.passed, 14);
        assert_eq!(r.failed, 2);
        assert_eq!(r.total, 16);
        assert!(r.failures.iter().any(|f| f.name.contains("does the thing")));
    }

    #[test]
    fn parse_pytest_failed_first_order() {
        // failed BEFORE passed — the summary order varies by outcome
        let text = "2 failed, 1 passed in 0.12s\nFAILED test_mod.py::test_sub - assert 0 == 1";
        let r = parse(text).unwrap();
        assert_eq!(r.runner, "pytest");
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed, 2);
        assert_eq!(r.total, 3);
        assert!(r
            .failures
            .iter()
            .any(|f| f.name == "test_sub" && f.file == "test_mod.py"));
    }

    #[test]
    fn parse_pytest_passed_only() {
        let r = parse("===== 5 passed in 1.20s =====").unwrap();
        assert_eq!(r.runner, "pytest");
        assert_eq!(r.passed, 5);
        assert_eq!(r.failed, 0);
        assert_eq!(r.total, 5);
    }

    #[test]
    fn parse_go_test_markers() {
        let text = "=== RUN   TestAdd\n--- PASS: TestAdd (0.00s)\n=== RUN   TestSub\n    calc_test.go:12: expected 3, got 4\n--- FAIL: TestSub (0.00s)\nFAIL\nFAIL\texample.com/calc\t0.5s\n";
        let r = parse(text).unwrap();
        assert_eq!(r.runner, "go");
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed, 1);
        assert_eq!(r.total, 2);
        let f = r.failures.iter().find(|f| f.name == "TestSub").unwrap();
        assert_eq!(f.file, "calc_test.go:12");
    }

    #[test]
    fn parse_bare_ok_lines_are_not_go() {
        // a python stdlib harness printing `ok <name>` / `FAIL <name>` (s457)
        let text = "ok test_add\nFAIL test_div AssertionError()\nok test_mod\n";
        let r = parse(text).unwrap();
        assert_eq!(r.runner, "ok-lines");
        assert_eq!(r.passed, 2);
        assert_eq!(r.failed, 1);
    }

    #[test]
    fn parse_go_test_package_level() {
        let text = "ok  \texample.com/good\t0.5s\nFAIL\texample.com/bad [build failed]\n";
        let r = parse(text).unwrap();
        assert_eq!(r.runner, "go");
        assert_eq!(r.passed, 1);
        assert_eq!(r.failed, 1);
        assert!(r.failures.iter().any(|f| f.text.contains("[build failed]")));
    }

    #[test]
    fn unknown_output_parses_to_none() {
        assert!(parse("hello world").is_none());
        assert!(parse("").is_none());
    }
}
