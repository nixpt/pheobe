//! Contract + integration tests, split by suite area (PHEOBE-21 LOC
//! budget): each section lives in its own file under `src/tests/`; shared
//! harness helpers (git runner, env knobs, repo fixture) live here.

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
mod update;
mod worker_route;

use std::path::Path;

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
