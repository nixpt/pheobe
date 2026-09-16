//! Contract + integration tests, split by suite area (PHEOBE-21 LOC
//! budget): each section lives in its own file under `src/tests/`; shared
//! harness helpers (git runner, env knobs, repo fixture) live here.
//!
//! `cli.rs` is a Cargo `[[test]]` target (spawns `CARGO_BIN_EXE_pheobe`),
//! not a submodule here.

mod agent_loop;
mod aging;
mod agy;
mod claude;
mod contract;
mod cursor;
mod e2e;
mod gates;
mod host;
mod knowledge;
mod opencode;
mod outcome;
mod p10;
mod scripted;
mod search;
mod structural;
mod update;
mod worker_route;

use std::path::{Path, PathBuf};

/// Write an executable `#!/bin/sh` shim at `dir/name` without ETXTBSY.
///
/// The dest inode is never open for write: we write+chmod a sibling tmp
/// and rename onto `name`. A parallel test's `fork` can inherit a write fd
/// on the tmp; `execve` of the dest then succeeds (issue 10).
///
/// `body` is the script after the shebang, unless it already starts with `#!`.
pub(crate) fn write_shim(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    let script = if body.starts_with("#!") {
        body.to_string()
    } else {
        format!("#!/bin/sh\n{body}")
    };
    std::fs::write(&tmp, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    std::fs::rename(&tmp, &path).unwrap();
    path
}

fn run_git(wt: &Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn git_repo_with(name: &str, content: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let n = SEQ.fetch_add(1, Ordering::SeqCst);
    let root = std::env::temp_dir().join(format!("pheobe-{}-{}-{}", name, std::process::id(), n));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    run_git(&root, &["init", "-q"]);
    run_git(&root, &["config", "user.email", "t@t"]);
    run_git(&root, &["config", "user.name", "t"]);
    std::fs::write(root.join(name), content).unwrap();
    run_git(&root, &["add", "."]);
    run_git(&root, &["commit", "-q", "-m", "init"]);
    root
}

fn claude_env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn set_env(k: &str, v: &str) {
    unsafe { std::env::set_var(k, v) };
}

fn del_env(k: &str) {
    unsafe { std::env::remove_var(k) };
}

fn worker_task() -> crate::task::Task {
    serde_json::from_str(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#).unwrap()
}

#[test]
fn write_shim_dest_is_executable() {
    let dir = std::env::temp_dir().join(format!("pheobe-shim-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let p = write_shim(&dir, "hello", "printf hi\n");
    let out = std::process::Command::new(&p).output().unwrap();
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi");
    std::fs::remove_dir_all(&dir).ok();
}
