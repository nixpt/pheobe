//! PHEOBE-26: host supervisor + issue 04 empty-worktree teardown.

use super::{claude_env_lock, del_env, git_repo_with, run_git, set_env};
use crate::task::Task;
use std::path::Path;

fn extra_worktrees(repo: &std::path::Path) -> Vec<String> {
    let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    run_git(&repo, &["worktree", "list", "--porcelain"])
        .lines()
        .filter_map(|l| l.strip_prefix("worktree "))
        .filter(|p| {
            Path::new(p)
                .canonicalize()
                .map(|c| c != repo)
                .unwrap_or(true)
        })
        .map(str::to_string)
        .collect()
}

#[test]
fn host_setup_then_finish_roundtrip() {
    let _g = claude_env_lock();
    del_env("PHEOBE_SANDBOX");
    let repo = git_repo_with("a.txt", "ok\n");
    let task: Task = serde_json::from_str(&format!(
        r#"{{"task":"Host setup finish roundtrip for PHEOBE-26","done_when":{{"type":"command","run":"test -f a.txt"}},"paths_allow":["a.txt"],"repo":"{}","worktree":true}}"#,
        repo.display()
    ))
    .unwrap();
    let setup = crate::host::setup(&task, Some("pheobe/p26-roundtrip")).unwrap();
    assert!(setup.ok);
    assert_eq!(setup.branch, "pheobe/p26-roundtrip");
    let wt = Path::new(&setup.worktree);
    assert!(wt.is_dir(), "worktree {}", setup.worktree);
    assert!(
        wt.join(".pheobe").join("plan.json").is_file(),
        "plan seeded"
    );
    let fin = crate::host::finish(&task, wt).unwrap();
    assert!(fin.ok, "finish should pass, got {fin:?}");
    assert!(fin.tests.passed);
    assert!(fin.violations.is_empty());
    crate::worktree::teardown(&repo, wt, &setup.branch).unwrap();
    assert!(extra_worktrees(&repo).is_empty());
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn host_setup_refuses_a_vague_task_before_provisioning() {
    let _g = claude_env_lock();
    del_env("PHEOBE_SANDBOX");
    let repo = git_repo_with("a.txt", "ok\n");
    let task: Task = serde_json::from_str(&format!(
        r#"{{"task":"polish the API","done_when":{{"type":"command","run":"true"}},"repo":"{}","worktree":true}}"#,
        repo.display()
    ))
    .unwrap();
    let err = format!(
        "{:#}",
        crate::host::setup(&task, Some("pheobe/p26-vague")).unwrap_err()
    );
    assert!(err.contains("vague ask"), "got {err}");
    assert!(
        extra_worktrees(&repo).is_empty(),
        "vague intake must not provision: {:?}",
        extra_worktrees(&repo)
    );
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn run_task_missing_endpoint_tears_down_the_empty_worktree() {
    let _g = claude_env_lock();
    del_env("PHEOBE_BASE_URL");
    del_env("PHEOBE_MODEL");
    del_env("PHEOBE_PROVIDER");
    del_env("PHEOBE_KEEP_WORKTREE");
    del_env("PHEOBE_SANDBOX");
    let repo = git_repo_with("a.txt", "ok\n");
    let task: Task = serde_json::from_str(&format!(
        r#"{{"task":"Issue 04 teardown when the endpoint is missing","done_when":{{"type":"command","run":"true"}},"paths_allow":["a.txt"],"repo":"{}","worktree":true}}"#,
        repo.display()
    ))
    .unwrap();
    let err = crate::run::run_task(&task, Some("pheobe/p26-teardown"), &|_| {}).unwrap_err();
    let msg = format!("{err:#}");
    assert!(
        msg.contains("PHEOBE_BASE_URL") || msg.contains("PHEOBE_MODEL"),
        "expected endpoint error, got {msg}"
    );
    assert!(
        extra_worktrees(&repo).is_empty(),
        "empty worktree must not remain: {:?}",
        extra_worktrees(&repo)
    );
    std::fs::remove_dir_all(&repo).ok();
}

#[test]
fn run_task_keep_worktree_leaves_the_failed_run_for_inspection() {
    let _g = claude_env_lock();
    del_env("PHEOBE_BASE_URL");
    del_env("PHEOBE_MODEL");
    del_env("PHEOBE_PROVIDER");
    del_env("PHEOBE_SANDBOX");
    set_env("PHEOBE_KEEP_WORKTREE", "1");
    let repo = git_repo_with("a.txt", "ok\n");
    let task: Task = serde_json::from_str(&format!(
        r#"{{"task":"Issue 04 keep flag leaves the kitchen","done_when":{{"type":"command","run":"true"}},"paths_allow":["a.txt"],"repo":"{}","worktree":true}}"#,
        repo.display()
    ))
    .unwrap();
    let err = crate::run::run_task(&task, Some("pheobe/p26-keep"), &|_| {});
    del_env("PHEOBE_KEEP_WORKTREE");
    assert!(err.is_err());
    let leftover = extra_worktrees(&repo);
    assert_eq!(
        leftover.len(),
        1,
        "keep flag leaves one worktree: {leftover:?}"
    );
    crate::worktree::teardown(&repo, Path::new(&leftover[0]), "pheobe/p26-keep").unwrap();
    std::fs::remove_dir_all(&repo).ok();
}
