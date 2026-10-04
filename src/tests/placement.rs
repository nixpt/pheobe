//! PHEOBE-51: worktree placement — inside a fleet repo (`.jagent/`), with
//! sibling links; a sibling of any other repo; old buckets falls back to git.

use crate::worktree::provision_with;
use std::path::Path;
use std::process::Command;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .status()
        .unwrap()
        .success();
    assert!(ok, "git {args:?}");
}

/// `<tmp>/repo` (+ `<tmp>/peer` sibling it path-deps on); `fleet` adds `.jagent/`.
fn setup(name: &str, fleet: bool) -> (std::path::PathBuf, std::path::PathBuf) {
    let outer = std::env::temp_dir().join(format!(
        "pheobe-place-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let repo = outer.join("repo");
    std::fs::create_dir_all(repo.join("crates/ai")).unwrap();
    std::fs::create_dir_all(outer.join("peer")).unwrap();
    std::fs::write(outer.join("peer/marker"), "peer").unwrap();
    std::fs::write(
        repo.join("crates/ai/Cargo.toml"),
        "[dependencies]\npeer = { path = \"../../../peer\" }\n",
    )
    .unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "init"]);
    if fleet {
        std::fs::create_dir_all(repo.join(".jagent")).unwrap();
    }
    (outer, repo)
}

#[test]
fn fleet_repo_worktree_is_inside_with_sibling_links() {
    let (outer, repo) = setup("fleet", true);
    let (wt, branch) = provision_with(&repo, "pheobe/t1", None).unwrap();
    assert_eq!(branch, "pheobe/t1");
    assert_eq!(wt, repo.join(".jagent/worktrees/pheobe-t1"));
    // crates/ai's ../../../peer resolves from inside the worktree
    let via = wt.join("crates/ai/../../../peer/marker");
    assert_eq!(std::fs::read_to_string(via).unwrap(), "peer");
    std::fs::remove_dir_all(&outer).ok();
}

#[test]
fn other_repos_keep_the_sibling_layout() {
    let (outer, repo) = setup("plain", false);
    let (wt, _) = provision_with(&repo, "pheobe/t2", None).unwrap();
    assert_eq!(
        std::fs::canonicalize(wt.parent().unwrap()).unwrap(),
        std::fs::canonicalize(&outer).unwrap()
    );
    std::fs::remove_dir_all(&outer).ok();
}

#[test]
fn buckets_without_path_flag_falls_back_to_git_in_repo() {
    let (outer, repo) = setup("oldbuckets", true);
    let shim = crate::tests::write_shim(
        &outer,
        "buckets",
        "echo \"error: unexpected argument '--path' found\" >&2\nexit 2\n",
    );
    let (wt, _) = provision_with(&repo, "pheobe/t3", Some(shim.to_str().unwrap())).unwrap();
    assert_eq!(wt, repo.join(".jagent/worktrees/pheobe-t3"));
    assert!(wt.join("crates/ai/Cargo.toml").exists());
    std::fs::remove_dir_all(&outer).ok();
}
