//! Engine parity helpers (PHEOBE-46): the per-run knobs every worker adapter
//! applies the same way, so the foreman can dispatch a routine task on any
//! engine and get the same guarantees.
//!
//! - **model**: `resolve_model(env, task)`, where the adapter's own env var beats the
//!   task field, the precedent PHEOBE-41 set for claude.
//! - **ttl**: `effective_timeout(env_secs, ttl)`, so the subprocess never outlives the task.
//! - **sandbox**: `command(..)` builds the child `Command` for a tier. Engines without a
//!   native sandbox (opencode, kimi, agy-moderate, claude) get pheobe's own bwrap
//!   confinement in `moderate`; `strict` is refused for any engine that needs the
//!   network to reach its model API; `free` is a plain subprocess. Engines with a
//!   native sandbox flag (cursor, codex, agy-strict) keep it and take the tier from
//!   `WorkerCtx` (not the env alone).

use crate::sandbox::Tier;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Adapter env override (non-empty) beats the task's `model` (PHEOBE-41 precedent).
pub(crate) fn resolve_model(env: Option<String>, task: Option<&str>) -> Option<String> {
    env.filter(|m| !m.trim().is_empty())
        .or_else(|| task.map(str::to_string))
        .filter(|m| !m.trim().is_empty())
}

/// min(env timeout, ttl) in whole seconds, never below 1 (PHEOBE-43).
pub(crate) fn effective_timeout(env_secs: u64, ttl: Option<Duration>) -> u64 {
    match ttl {
        Some(t) => env_secs.min(t.as_secs().max(1)),
        None => env_secs,
    }
}

/// `$VAR` parsed as seconds, else the default.
pub(crate) fn env_secs(var: &str, default: u64) -> u64 {
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .unwrap_or(default)
}

pub(crate) fn plain(bin: &str, argv: &[String]) -> Command {
    let mut c = Command::new(bin);
    c.args(argv);
    c
}

pub(crate) fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|d| d.join(name))
            .find(|p| p.is_file())
    })
}

/// Host paths the moderate sandbox needs besides the worktree.
#[derive(Debug, Default, Clone)]
pub(crate) struct Mounts {
    pub home: Option<PathBuf>,
    /// the worktree's own git dir (`.git/worktrees/<name>`), writable
    pub git_dir: Option<PathBuf>,
    /// the shared git common dir, writable (engine commits land here)
    pub git_common: Option<PathBuf>,
    /// the main checkout's parent, so sibling `../x` path-deps resolve (read-only)
    pub repo_parent: Option<PathBuf>,
    pub cargo_target: Option<PathBuf>,
}

impl Mounts {
    pub(crate) fn for_worktree(wt: &Path) -> Self {
        let git = |arg: &str| {
            Command::new("git")
                .arg("-C")
                .arg(wt)
                .args(["rev-parse", "--path-format=absolute", arg])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| PathBuf::from(String::from_utf8_lossy(&o.stdout).trim().to_string()))
        };
        let git_common = git("--git-common-dir");
        Mounts {
            home: std::env::var_os("HOME").map(PathBuf::from),
            git_dir: git("--git-dir"),
            repo_parent: git_common
                .as_deref()
                .and_then(Path::parent)
                .and_then(Path::parent)
                .map(Path::to_path_buf),
            git_common,
            cargo_target: std::env::var_os("CARGO_TARGET_DIR").map(PathBuf::from),
        }
    }
}

/// Per-engine state under `$HOME` that must stay writable inside the moderate
/// sandbox (auth refresh, session store, caches). Everything else in `$HOME`
/// is read-only.
pub(crate) const CLAUDE_HOME_RW: &[&str] = &[".claude", ".claude.json"];
pub(crate) const OPENCODE_HOME_RW: &[&str] = &[
    ".local/share/opencode",
    ".local/state/opencode",
    ".cache/opencode",
    ".config/opencode",
];
/// kimi = cece on this fleet (kimi-cli fork): its state lives in ~/.cece; ~/.kimi for upstream.
pub(crate) const KIMI_HOME_RW: &[&str] = &[".cece", ".kimi"];
pub(crate) const AGY_HOME_RW: &[&str] = &[".gemini"];

/// bwrap argv for a moderate run: network shared, writes confined to the
/// worktree (+ its repo's git store, the engine's own state dirs under
/// `$HOME`, `$CARGO_TARGET_DIR`).
pub(crate) fn moderate_bwrap_args(
    bin: &str,
    argv: &[String],
    wt: &Path,
    m: &Mounts,
    home_rw: &[&str],
) -> Vec<String> {
    let mut a: Vec<String> = vec!["--unshare-pid".into(), "--die-with-parent".into()];
    let mut add = |flag: &str, p: &Path| {
        let p = p.display().to_string();
        a.extend([flag.to_string(), p.clone(), p]);
    };
    for d in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc", "/opt"] {
        add("--ro-bind-try", Path::new(d)); // /etc: DNS + TLS roots for the API
    }
    // pseudo-fs + private /tmp FIRST: bwrap applies mounts in order, so a
    // later --tmpfs /tmp would shadow a worktree (or bin) that lives under /tmp
    a.extend([
        "--dev".into(),
        "/dev".into(),
        "--proc".into(),
        "/proc".into(),
    ]);
    a.extend(["--tmpfs".into(), "/tmp".into()]);
    let mut add = |flag: &str, p: &Path| {
        let p = p.display().to_string();
        a.extend([flag.to_string(), p.clone(), p]);
    };
    // an absolute engine binary must stay reachable wherever it lives
    if let Some(dir) = Path::new(bin)
        .is_absolute()
        .then(|| Path::new(bin).parent())
        .flatten()
    {
        add("--ro-bind-try", dir);
    }
    if let Some(h) = &m.home {
        add("--ro-bind-try", h);
        for rel in home_rw {
            add("--bind-try", &h.join(rel));
        }
    }
    if let Some(ro) = &m.repo_parent {
        add("--ro-bind-try", ro);
    }
    // the git store is WRITABLE: the worker prompt tells the engine to commit
    // its work (agent.rs), and a commit writes objects + refs into the common
    // dir, not only the per-worktree gitdir (s463 live haiku run).
    for rw in [&m.git_common, &m.git_dir, &m.cargo_target]
        .into_iter()
        .flatten()
    {
        add("--bind-try", rw);
    }
    add("--bind", wt);
    a.extend(["--chdir".into(), wt.display().to_string()]);
    a.push(bin.to_string());
    a.extend(argv.iter().cloned());
    a
}

/// The child command for an engine without a native sandbox.
///
/// `tier == None` means a direct `Worker::run()` caller outside `run_worker`:
/// unsandboxed, exactly as before PHEOBE-43/46. `run_worker` always resolves one.
pub(crate) fn command(
    engine: &str,
    bin: &str,
    argv: &[String],
    wt: &Path,
    tier: Option<&Tier>,
    home_rw: &[&str],
) -> anyhow::Result<Command> {
    Ok(match tier {
        None | Some(Tier::Free) => plain(bin, argv),
        Some(Tier::Strict) => anyhow::bail!(strict_refusal(engine)),
        Some(Tier::Moderate) => match which("bwrap") {
            Some(bwrap) => {
                let mut c = Command::new(bwrap);
                c.args(moderate_bwrap_args(
                    bin,
                    argv,
                    wt,
                    &Mounts::for_worktree(wt),
                    home_rw,
                ));
                c
            }
            None => {
                eprintln!(
                    "pheobe: {engine} worker: bwrap not found — moderate tier degrades to a \
                     plain subprocess"
                );
                plain(bin, argv)
            }
        },
    })
}

/// strict unshares the network; an engine that calls a hosted model API
/// cannot run there. Refuse loudly rather than run it unconfined.
pub(crate) fn strict_refusal(engine: &str) -> String {
    format!(
        "{engine} worker: sandbox 'strict' is not supported — the {engine} CLI needs the \
         network to reach its model API and strict unshares it. Use PHEOBE_SANDBOX=moderate \
         (bwrap, network on, worktree-only writes) or free."
    )
}

/// The tier an adapter applies: the run's resolved tier (`WorkerCtx`, i.e.
/// `PHEOBE_SANDBOX` → task → moderate), else, for a direct `run()` caller,
/// `PHEOBE_SANDBOX` (default moderate). Native-sandbox adapters use this so
/// the task's `sandbox` field reaches them, not only the env.
pub(crate) fn tier_or_env(ctx_tier: Option<&Tier>) -> anyhow::Result<Tier> {
    match ctx_tier {
        Some(t) => Ok(t.clone()),
        None => {
            let name = std::env::var("PHEOBE_SANDBOX").unwrap_or_else(|_| "moderate".into());
            Tier::from_name(&name).map_err(|_| {
                anyhow::anyhow!(
                    "unknown PHEOBE_SANDBOX tier '{}' — expected strict | moderate | free \
                     (defaults to moderate when unset)",
                    name.trim()
                )
            })
        }
    }
}

/// Wait for the engine up to `secs`; on timeout KILL it (and reap) before
/// erroring. `wait_timeout` only reports the timeout — before PHEOBE-46 every
/// adapter errored "timed out … and was killed" while the engine kept running
/// (and spending, and possibly committing) in the background.
pub(crate) fn wait_or_kill(
    child: &mut std::process::Child,
    secs: u64,
    engine: &str,
    bin: &str,
    env_var: &str,
) -> anyhow::Result<std::process::ExitStatus> {
    use anyhow::Context;
    match wait_timeout::ChildExt::wait_timeout(child, Duration::from_secs(secs))
        .with_context(|| format!("{engine} worker: waiting on '{bin}' failed"))?
    {
        Some(status) => Ok(status),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!(
                "{engine} worker: '{bin}' timed out after {secs}s and was killed \
                 (set {env_var} to adjust; the task ttl caps it too; the aging ladder in \
                 run_worker judges the run separately)"
            )
        }
    }
}
