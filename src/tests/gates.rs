//! .jagent/issues regression tests (verified live on Zen, s456): the
//! porcelain parse, the state-dir exemption + byproduct staging, the
//! branch-suffix rule — plus the plan depends_on edge shape.

use super::git_repo_with;
use crate::plan;

/// Issue 01: single modified file as the FIRST porcelain line — ` M calc.py`
/// must parse to `calc.py`, not `alc.py`.
#[test]
fn issue01_first_porcelain_line_not_mangled() {
    let root = git_repo_with("calc.py", "def add(a,b):\n    return a-b\n");
    std::fs::write(root.join("calc.py"), "def add(a,b):\n    return a+b\n").unwrap();

    let (violations, _by) =
        crate::worktree::check_allowlist(&root, &["calc.py".to_string()]).unwrap();
    assert!(
        violations.is_empty(),
        "edited allowlisted file must pass, got: {violations:?}"
    );

    // a genuinely outside edit (tracked file, e.g. via bash sed) is caught
    // with its FULL name; untracked junk would be a byproduct instead
    let other = git_repo_with("calc.py", "x");
    std::fs::write(other.join("other.py"), "z").unwrap();
    crate::tests::run_git(&other, &["add", "."]);
    crate::tests::run_git(&other, &["commit", "-q", "-m", "add other"]);
    std::fs::write(other.join("other.py"), "w").unwrap();
    let (violations, _) =
        crate::worktree::check_allowlist(&other, &["calc.py".to_string()]).unwrap();
    assert_eq!(violations, vec!["other.py".to_string()]);
    let _ = std::fs::remove_dir_all(&root);
}

/// Issue 02: pheobe's own state dir is never a violation; bash-run byproducts
/// are reported separately and staged OUT of the commit.
#[test]
fn issue02_state_dir_exempt_and_byproducts_staged_out() {
    let root = git_repo_with("calc.py", "def add(a,b):\n    return a-b\n");
    std::fs::write(root.join("calc.py"), "def add(a,b):\n    return a+b\n").unwrap();
    std::fs::create_dir_all(root.join(".pheobe")).unwrap();
    std::fs::write(root.join(".pheobe/plan.json"), "{}").unwrap();
    std::fs::create_dir_all(root.join("__pycache__")).unwrap();
    std::fs::write(root.join("__pycache__/junk.py"), "x").unwrap();

    let (violations, byproducts) =
        crate::worktree::check_allowlist(&root, &["calc.py".to_string()]).unwrap();
    assert!(
        violations.is_empty(),
        ".pheobe must be exempt, got: {violations:?}"
    );
    assert_eq!(byproducts, vec!["__pycache__/".to_string()]);

    // commit stages ONLY the allowlisted file — the byproduct stays out
    let sha = crate::worktree::commit(&root, "t", "fix add", &["calc.py".to_string()]).unwrap();
    assert!(!sha.is_empty());
    let committed = crate::tests::run_git(&root, &["show", "--name-only", "--format=", "HEAD"]);
    assert!(committed.contains("calc.py"), "got: {committed}");
    assert!(
        !committed.contains("__pycache__"),
        "byproduct staged in: {committed}"
    );
    assert!(
        !committed.contains(".pheobe"),
        "state staged in: {committed}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// Issue 03: an existing `pheobe/<slug>` branch never gets silently reused.
#[test]
fn issue03_existing_branch_suffixed_not_reused() {
    let root = git_repo_with("f.txt", "a\n");
    crate::tests::run_git(&root, &["branch", "pheobe/demo"]);
    let b = crate::worktree::next_free_branch(&root, "pheobe/demo").unwrap();
    assert_eq!(b, "pheobe/demo-2");
    crate::tests::run_git(&root, &["branch", "pheobe/demo-2"]);
    let b2 = crate::worktree::next_free_branch(&root, "pheobe/demo").unwrap();
    assert_eq!(b2, "pheobe/demo-3");
    // free base comes back untouched
    let b3 = crate::worktree::next_free_branch(&root, "pheobe/fresh").unwrap();
    assert_eq!(b3, "pheobe/fresh");
    let _ = std::fs::remove_dir_all(&root);
}

/// Loops/graphs research: plan steps gain optional `depends_on` edges —
/// a parent that fanned out several pheobe nodes can consume this plan as
/// a sub-DAG. Validation lives in the plan_tracker tool, not an engine.
#[test]
fn plan_steps_carry_dependency_edges() {
    let steps: Vec<plan::Step> = serde_json::from_str(
        r#"[
        {"desc": "add module", "status": "done"},
        {"desc": "wire CLI flag", "status": "todo", "depends_on": [0]},
        {"desc": "tests", "status": "todo", "depends_on": [0, 1]}
    ]"#,
    )
    .unwrap();
    assert!(steps[0].depends_on.is_empty());
    assert_eq!(steps[1].depends_on, vec![0]);
    assert_eq!(steps[2].depends_on, vec![0, 1]);
}
