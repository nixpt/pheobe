//! PHEOBE-10: plan_tracker depends_on validation + the checkpoint
//! create/restore roundtrip.

use super::git_repo_with;
use crate::plan;

fn p10_step(desc: &str, status: &str, deps: Vec<u32>) -> plan::Step {
    plan::Step {
        desc: desc.into(),
        status: status.into(),
        doubts: vec![],
        depends_on: deps,
    }
}

#[test]
fn p10_plan_depends_on_validation() {
    use plan::validate_steps;

    // valid DAG passes
    assert!(validate_steps(&[
        p10_step("a", "done", vec![]),
        p10_step("b", "done", vec![0]),
        p10_step("c", "todo", vec![0, 1]),
    ])
    .is_ok());

    // out-of-range index
    let e = validate_steps(&[p10_step("a", "todo", vec![5])]).unwrap_err();
    assert!(e.contains("out of range"), "{e}");

    // self-reference
    let e = validate_steps(&[p10_step("a", "todo", vec![0])]).unwrap_err();
    assert!(e.contains("depends on itself"), "{e}");

    // cycle surfaces as cycle_detected with the path
    let e = validate_steps(&[
        p10_step("a", "todo", vec![1]),
        p10_step("b", "todo", vec![2]),
        p10_step("c", "todo", vec![0]),
    ])
    .unwrap_err();
    assert!(e.contains("cycle_detected"), "{e}");

    // done step with an undone predecessor is refused; blockers are listed
    let e = validate_steps(&[
        p10_step("a", "doing", vec![]),
        p10_step("b", "done", vec![0]),
    ])
    .unwrap_err();
    assert!(e.contains("cannot be marked done"), "{e}");
    assert!(e.contains("blocking predecessors"), "{e}");
    assert!(e.contains("'a'"), "blocker desc in refusal: {e}");
}

// ── PHEOBE-10: checkpoints ───────────────────────────────────────────────────

#[test]
fn p10_checkpoint_create_restore_roundtrip() {
    use crate::checkpoint as cp;
    let root = git_repo_with("f.txt", "one\n");

    // clean tree → recorded, no stash to restore
    cp::create(&root, "clean-point").unwrap();
    let l = cp::list(&root).unwrap();
    assert_eq!(l.len(), 1);
    assert!(l[0].stash.is_none());
    assert!(cp::restore(&root, "clean-point").is_err());

    // dirty state at checkpoint time
    std::fs::write(root.join("f.txt"), "two\n").unwrap();
    let msg = cp::create(&root, "before").unwrap();
    assert!(msg.contains("before") && msg.contains("stash="), "{msg}");

    // the failed-iteration shape: edit backwards, then restore brings the
    // checkpoint state back
    std::fs::write(root.join("f.txt"), "one\n").unwrap();
    cp::restore(&root, "before").unwrap();
    assert_eq!(
        std::fs::read_to_string(root.join("f.txt")).unwrap(),
        "two\n"
    );

    // prune keeps the newest 5 (upsert'd names stay distinct)
    for i in 0..7 {
        cp::create(&root, &format!("bulk-{i}")).unwrap();
    }
    let msg = cp::prune(&root, 5).unwrap();
    assert!(msg.contains("pruned 4"), "{msg}");
    let l = cp::list(&root).unwrap();
    assert_eq!(l.len(), 5);
    assert!(l.iter().any(|c| c.name == "bulk-6"));
    assert!(!l.iter().any(|c| c.name == "clean-point"));

    // bad names refused
    assert!(cp::create(&root, "bad name!").is_err());

    let _ = std::fs::remove_dir_all(&root);
}
