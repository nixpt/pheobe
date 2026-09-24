//! PHEOBE-48: `.pheobe/` stays out of commits an ENGINE makes itself.
//! s463: a claude-sdk engine ran `git add -A && git commit` and swept
//! `.pheobe/plan.json` into foreman-v9 57d13fe — pheobe's own `:!/.pheobe`
//! pathspec only guards pheobe's commit, not the engine's.

use super::{git_repo_with, run_git};
use crate::worktree;

fn exclude_count(wt: &std::path::Path) -> usize {
    let p = worktree::ensure_pheobe_excluded(wt).unwrap();
    std::fs::read_to_string(p)
        .unwrap()
        .lines()
        .filter(|l| l.trim() == worktree::EXCLUDE_LINE)
        .count()
}

#[test]
fn provision_writes_the_exclude_line_once_and_is_idempotent() {
    let repo = git_repo_with("README", "x\n");
    let (wt, b) = worktree::provision(&repo, "agent/t/p48-a").unwrap();
    assert_eq!(exclude_count(&wt), 1);
    // a second run on the same clone shares the exclude file — still one line
    let (wt2, b2) = worktree::provision(&repo, "agent/t/p48-b").unwrap();
    assert_eq!(exclude_count(&wt2), 1);
    assert_eq!(exclude_count(&wt), 1);
    let _ = worktree::teardown(&repo, &wt, &b);
    let _ = worktree::teardown(&repo, &wt2, &b2);
}

#[test]
fn an_engines_add_all_commit_does_not_pick_up_pheobe_state() {
    let repo = git_repo_with("README", "x\n");
    let (wt, b) = worktree::provision(&repo, "agent/t/p48-c").unwrap();
    std::fs::create_dir_all(wt.join(".pheobe")).unwrap();
    std::fs::write(wt.join(".pheobe/plan.json"), "{}\n").unwrap();
    std::fs::write(wt.join("README"), "x\ny\n").unwrap();
    run_git(&wt, &["add", "-A"]); // what the engine did in s463
    run_git(
        &wt,
        &[
            "-c",
            "user.email=e@e",
            "-c",
            "user.name=e",
            "commit",
            "-q",
            "-m",
            "engine",
        ],
    );
    let files = run_git(&wt, &["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files.trim(), "README", "committed: {files}");
    assert!(
        wt.join(".pheobe/plan.json").exists(),
        "sidecar state must survive, just untracked"
    );
    let _ = worktree::teardown(&repo, &wt, &b);
}

#[test]
fn the_tracked_gitignore_is_untouched() {
    let repo = git_repo_with(".gitignore", "/target\n");
    let (wt, b) = worktree::provision(&repo, "agent/t/p48-d").unwrap();
    assert_eq!(
        std::fs::read_to_string(wt.join(".gitignore")).unwrap(),
        "/target\n"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join(".gitignore")).unwrap(),
        "/target\n"
    );
    assert_eq!(
        run_git(&wt, &["status", "--porcelain"]),
        "",
        "exclude must not dirty the tree"
    );
    let _ = worktree::teardown(&repo, &wt, &b);
}

#[test]
fn a_forced_pheobe_commit_is_named_for_the_doubts() {
    let repo = git_repo_with("README", "x\n");
    let (wt, b) = worktree::provision(&repo, "agent/t/p48-e").unwrap();
    let base = worktree::head_sha(&wt).unwrap();
    assert!(worktree::commits_touching_pheobe(&wt, &base)
        .unwrap()
        .is_empty());
    std::fs::create_dir_all(wt.join(".pheobe")).unwrap();
    std::fs::write(wt.join(".pheobe/plan.json"), "{}\n").unwrap();
    run_git(&wt, &["add", "-f", ".pheobe/plan.json"]);
    run_git(
        &wt,
        &[
            "-c",
            "user.email=e@e",
            "-c",
            "user.name=e",
            "commit",
            "-q",
            "-m",
            "forced",
        ],
    );
    let hits = worktree::commits_touching_pheobe(&wt, &base).unwrap();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert!(hits[0].ends_with(" .pheobe/plan.json"), "{hits:?}");
    let _ = worktree::teardown(&repo, &wt, &b);
}
