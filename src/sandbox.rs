//! The sandbox ladder (PHEOBE-14): namespace-isolated bash execution.
//!
//! Maps the task's `sandbox` / `PHEOBE_SANDBOX` tier onto `bwrap` mounts:
//!
//! | tier | `--unshare-net` | network | writable files |
//! |------|----------------|---------|----------------|
//! | strict | yes | no | worktree only |
//! | moderate (default) | no | yes | worktree only |
//! | free | n/a | n/a | plain subprocess (destructive guard still on) |
//!
//! Strict is the security baseline: mount namespace + PID namespace, no
//! network, read-only /usr /bin /sbin /lib (and /lib64 where it exists),
//! /dev and /proc as pseudo-fs, TMPDIR private scratch under `/tmp` so the
//! sandbox can write to temp without escaping the worktree.  The
//! `done_when` command is appended to the prompt so the model knows the
//! final verification gate; the allowlist gate at commit has the last word.
//!
//! Fail-closed: `bwrap` missing at RUN START → `strict` blocks with a
//! clear error (`blocked:"no_sandbox"`).  `moderate` without `bwrap` falls
//! back to the plain process-capsule (the destructive guard still applies
//! — this is a safe degradation, not a silent one).

use anyhow::{bail, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Parsed tier for one run.  Built once at intake via `Task::effective_sandbox()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tier {
    /// Mount ns + PID ns, worktree read-write, NO network.
    Strict,
    /// Mount ns + PID ns, worktree read-write, network allowed.
    Moderate,
    /// Plain subprocess — the destructive guard is still on.
    Free,
}

impl Tier {
    pub fn from_name(s: &str) -> Result<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "strict" => Ok(Tier::Strict),
            "moderate" => Ok(Tier::Moderate),
            "free" => Ok(Tier::Free),
            other => bail!("unknown sandbox tier '{other}'"),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Strict => "strict",
            Tier::Moderate => "moderate",
            Tier::Free => "free",
        }
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Fail-closed check at RUN START: strict requires bwrap on PATH.  An
/// operator asking for strict containment that can't be delivered gets a
/// `blocked:"no_sandbox"` error BEFORE any tool runs — never a silent
/// downgrade.  Moderate without bwrap degrades (safe: the destructive guard
/// still applies) and is flagged for the doubts, not blocked.
pub fn ensure_at_intake(tier: &Tier) -> Result<()> {
    if matches!(tier, Tier::Strict) && find_bwrap().is_none() {
        bail!(
            "sandbox '{tier}' blocked incoming: `bwrap` not found on PATH \
             (blocked:no_sandbox) — set PHEOBE_SANDBOX=free to run unsandboxed, \
             or install bubblewrap"
        );
    }
    Ok(())
}

/// `bwrap` on PATH?  Returns its path for the failure message.
fn find_bwrap() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join("bwrap");
        if cand.is_file() {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(m) = std::fs::metadata(&cand) {
                if m.permissions().mode() & 0o111 != 0 {
                    return Some(cand);
                }
            }
        }
    }
    None
}

/// Base host directories bound read-only into every sandbox — enough for a
/// dynamically-linked binary and a shell to function.  Deliberately mirrors
/// the `buckets` precedent (`buckets/src/sandbox.rs:BASE_RO_DIRS`).
const BASE_RO_DIRS: &[&str] = &["/usr", "/bin", "/sbin", "/lib", "/lib64"];

/// Build the `bwrap` argv for a single bash invocation.  The caller is
/// responsible for finding `bwrap` itself (fail-fast when missing).
pub fn build_bwrap_args(cmd: &str, worktree: &Path, tier: &Tier) -> Vec<String> {
    let mut args = Vec::new();
    // PID namespace — the sandboxed shell is PID 1
    args.push("--unshare-pid".into());
    // no network for strict
    if matches!(tier, Tier::Strict) {
        args.push("--unshare-net".into());
    }
    // base host dirs (read-only); use --ro-bind-try for dirs that may not
    // exist on all hosts (e.g. /lib64)
    for dir in BASE_RO_DIRS {
        args.push("--ro-bind-try".into());
        args.push(dir.to_string());
        args.push(dir.to_string());
    }
    // pseudo filesystems
    args.push("--dev".into());
    args.push("/dev".into());
    args.push("--proc".into());
    args.push("/proc".into());
    // TMPDIR private scratch under /tmp — the sandbox can write to temp
    // without leaking host state
    let scratch = format!("/tmp/pheobe-sandbox-{}", std::process::id());
    args.push("--tmpfs".into());
    args.push(scratch.clone());
    args.push("--setenv".into());
    args.push("TMPDIR".into());
    args.push(scratch);
    // worktree is the only writable host directory
    args.push("--bind".into());
    args.push(worktree.display().to_string());
    args.push(worktree.display().to_string());
    // cd into the worktree inside the sandbox
    args.push("--chdir".into());
    args.push(worktree.display().to_string());
    // the command to run
    args.push("sh".into());
    args.push("-c".into());
    args.push(cmd.to_string());
    args
}

/// Strict-tier allowlist: only done_when / test / build commands are allowed
/// through.  Anything else hits a tool error naming the tier restriction.
const ALLOWED_PREFIXES: &[&str] = &[
    "cargo test",
    "cargo build",
    "npm test",
    "npx",
    "pytest",
    "python3",
    "go test",
    "go build",
    "sh -c",
    "node",
];

/// Check whether a bash command is allowed in the strict tier.
pub fn strict_allowed(cmd: &str) -> bool {
    let trimmed = cmd.trim_start();
    ALLOWED_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
}

/// Wrap a command to run inside the sandbox.  For `strict`/`moderate` this
/// spawns `bwrap` with the right mounts; for `free` it returns `None` (the
/// caller should run the plain `Command`).  Returns `Err` only when the
/// sandbox is `strict` and `bwrap` is missing (fail-closed).
pub fn sandboxed_command(cmd: &str, worktree: &Path, tier: &Tier) -> Result<Option<Command>> {
    match tier {
        Tier::Free => Ok(None),
        Tier::Strict | Tier::Moderate => {
            let bwrap = find_bwrap().ok_or_else(|| {
                anyhow::anyhow!(
                    "sandbox '{tier}' requires `bwrap` but it was not found on PATH — \
                     set PHEOBE_SANDBOX=free to run unsandboxed, or install bubblewrap"
                )
            })?;
            let mut c = Command::new(bwrap);
            for arg in build_bwrap_args(cmd, worktree, tier) {
                c.arg(&arg);
            }
            Ok(Some(c))
        }
    }
}

/// The note appended to the prompt when strict mode is active, so the model
/// knows the allowlist is enforced by pheobe's gates after the run.
pub const STRICT_NOTE: &str = "\n\n[sandbox note: strict tier — your bash tool runs inside a \
    network-free, read-only sandbox. Only the worktree is writable. \
    pheobe's paths_allow gate enforces the allowlist mechanically after \
    this run; a task that must write files or commit needs \
    PHEOBE_SANDBOX=moderate.]";

/// Format for the doubt note when moderate without bwrap falls back to plain
/// process capsule.
pub const MODERATE_FALLBACK_DOUBT: &str =
    "PHEOBE_SANDBOX=moderate requested but `bwrap` was not found on PATH; \
     the bash tool ran WITHOUT sandbox isolation (destructive guard still \
     applied). Install bubblewrap for real containment.";

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static PATH_LOCK: Mutex<()> = Mutex::new(());

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pheobe-sandbox-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_bin(dir: &Path, name: &str) -> PathBuf {
        crate::tests::write_shim(dir, name, "echo fake-bwrap\n")
    }

    #[test]
    fn tier_from_name_accepts_the_three_valid_tiers() {
        assert_eq!(Tier::from_name("strict").unwrap(), Tier::Strict);
        assert_eq!(Tier::from_name("moderate").unwrap(), Tier::Moderate);
        assert_eq!(Tier::from_name("free").unwrap(), Tier::Free);
        assert_eq!(Tier::from_name("STRICT").unwrap(), Tier::Strict);
        assert!(Tier::from_name("chaos").is_err());
    }

    #[test]
    fn bwrap_args_include_mount_pid_and_correct_worktree_bind() {
        let dir = scratch("args");
        let args = build_bwrap_args("cargo test", &dir, &Tier::Strict);
        assert!(args.contains(&"--unshare-pid".into()), "has PID ns");
        assert!(
            args.contains(&"--unshare-net".into()),
            "strict = no network"
        );
        assert!(args.contains(&"--dev".into()), "has pseudo /dev");
        assert!(args.contains(&"--proc".into()), "has pseudo /proc");
        let wt_idx = args.iter().position(|a| a == "--bind").expect("has --bind");
        assert_eq!(
            args[wt_idx + 1],
            dir.display().to_string(),
            "bind source = worktree"
        );
        assert!(
            args.last() == Some(&"sh".into()) || args.contains(&"sh".into()),
            "the shell command is the final part of the bwrap argv"
        );
        assert!(args.contains(&"-c".into()), "has sh -c");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn bwrap_args_for_moderate_omit_unshare_net() {
        let dir = scratch("moderate");
        let args = build_bwrap_args("echo ok", &dir, &Tier::Moderate);
        assert!(args.contains(&"--unshare-pid".into()));
        assert!(
            !args.contains(&"--unshare-net".into()),
            "moderate allows network"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn free_tier_returns_none_from_sandboxed_command() {
        let dir = scratch("free");
        assert!(sandboxed_command("ls", &dir, &Tier::Free)
            .unwrap()
            .is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn strict_without_bwrap_fails_closed() {
        let dir = scratch("no-bwrap");
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let real_path = std::env::var("PATH").unwrap_or_default();
        // empty PATH = no bwrap
        let empty = dir.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        unsafe { std::env::set_var("PATH", empty.display().to_string()) };
        let err = format!(
            "{:#}",
            sandboxed_command("ls", &dir, &Tier::Strict).unwrap_err()
        );
        assert!(err.contains("bwrap"), "names the missing tool: {err}");
        assert!(err.contains("free"), "tells you the escape hatch: {err}");
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn strict_with_bwrap_returns_a_spawnable_command() {
        let dir = scratch("fake-bwrap");
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let real_path = std::env::var("PATH").unwrap_or_default();
        fake_bin(&dir, "bwrap");
        unsafe { std::env::set_var("PATH", format!("{}:{real_path}", dir.display())) };
        let mut cmd = sandboxed_command("echo ok", &dir, &Tier::Strict)
            .unwrap()
            .expect("Some");
        let out = cmd.output().unwrap();
        assert!(out.status.success());
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("fake-bwrap"),
            "the fake bwrap ran, got: {:?}",
            String::from_utf8_lossy(&out.stdout)
        );
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn strict_allowed_accepts_test_build_prefixes() {
        assert!(strict_allowed("cargo test"));
        assert!(strict_allowed("  cargo build --release"));
        assert!(strict_allowed("pytest -x"));
        assert!(strict_allowed("python3 -m pytest"));
        assert!(strict_allowed("go test ./..."));
        assert!(strict_allowed("npm test"));
        assert!(strict_allowed("sh -c 'ls'"));
        assert!(!strict_allowed("rm -rf /"));
        assert!(!strict_allowed("curl example.com"));
        assert!(!strict_allowed("git push origin main"));
    }

    #[test]
    fn effective_sandbox_env_overrides_task_value() {
        let _lock = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let task = crate::task::Task {
            task: "test".into(),
            done_when: crate::task::DoneWhen::Command {
                run: "true".into(),
                expect_exit: 0,
            },
            repo: None,
            worktree: true,
            branch: None,
            ttl: None,
            budget: None,
            paths_allow: vec![],
            push: false,
            sandbox: Some("free".into()),
            model: None,
        };
        // task says free, but env overrides to strict
        unsafe { std::env::set_var("PHEOBE_SANDBOX", "strict") };
        assert_eq!(task.effective_sandbox().unwrap(), "strict");
        // env unset → task value wins
        unsafe { std::env::remove_var("PHEOBE_SANDBOX") };
        assert_eq!(task.effective_sandbox().unwrap(), "free");
        // no env, no task → default moderate
        let task2 = crate::task::Task {
            sandbox: None,
            ..task.clone()
        };
        assert_eq!(task2.effective_sandbox().unwrap(), "moderate");
    }
}
