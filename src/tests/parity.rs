//! PHEOBE-46 engine parity: truthful usage (usd + turns) from every worker,
//! engine-error-after-commit is a failed run (exit 1) not "never ran" (exit 2),
//! the task's `provider` picks the engine (env still wins), and the shared
//! engine helpers (model / ttl / sandbox) behave the same for every adapter.
//! Per-engine argv (model flags) lives in each engine's own test module under
//! that module's env lock.

use super::{claude_env_lock, del_env, run_git, set_env};
use crate::agent::{run_worker, LoopCfg};
use crate::sandbox::Tier;
use crate::task::Task;
use crate::worker::{Worker, WorkerOutcome};
use std::path::Path;
use std::time::Duration;

struct Mock(Result<WorkerOutcome, String>);

impl Worker for Mock {
    fn run(&self, _prompt: &str, _wt: &Path) -> anyhow::Result<WorkerOutcome> {
        match &self.0 {
            Ok(o) => Ok(WorkerOutcome {
                final_text: o.final_text.clone(),
                tokens: o.tokens,
                usd: o.usd,
                json_tail: o.json_tail.clone(),
                turns: o.turns,
            }),
            Err(e) => anyhow::bail!("{e}"),
        }
    }
}

fn task(json: &str) -> Task {
    serde_json::from_str(json).unwrap()
}

// ── report truth at the run_worker seam ─────────────────────────────────────
#[test]
fn worker_usd_and_turns_reach_the_run_outcome() {
    let w = Mock(Ok(WorkerOutcome {
        final_text: "done".into(),
        tokens: Some(900),
        usd: Some(0.12),
        json_tail: None,
        turns: Some(3),
    }));
    let t = task(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#);
    let wt = std::env::temp_dir();
    let out = run_worker(&w, &t, &wt, "p1", "", &[], &LoopCfg::default()).unwrap();
    assert!(out.ok);
    assert_eq!(
        out.usage.usd,
        Some(0.12),
        "the engine's cost is no longer dropped"
    );
    assert_eq!(out.usage.turns, 3, "real turns, not a flat 1");
    assert!(out.engine_error.is_none());
}

#[test]
fn a_failing_engine_is_an_outcome_not_an_early_error() {
    let w = Mock(Err("exited with exit status: 1: boom".into()));
    let t = task(r#"{"task":"x","done_when":{"type":"command","run":"true"}}"#);
    let out = run_worker(
        &w,
        &t,
        &std::env::temp_dir(),
        "p2",
        "",
        &[],
        &LoopCfg::default(),
    )
    .expect("run_worker must not `?` the engine error away");
    assert!(!out.ok);
    assert!(
        out.blocked
            .as_deref()
            .unwrap()
            .starts_with("engine failed:"),
        "{:?}",
        out.blocked
    );
    assert!(out.engine_error.as_deref().unwrap().contains("boom"));
}

// ── run_task: the exit class of an engine error ─────────────────────────────
fn repo() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static SEQ: AtomicU32 = AtomicU32::new(0);
    let root = std::env::temp_dir().join(format!(
        "pheobe-parity-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&root);
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q"]);
    run_git(&repo, &["config", "user.email", "t@t"]);
    run_git(&repo, &["config", "user.name", "t"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    run_git(&repo, &["add", "-A"]);
    run_git(&repo, &["commit", "-qm", "root"]);
    repo
}

/// A fake engine binary: optionally commits `engine.txt` in its cwd (the
/// worktree), then prints `stdout` and exits with `code`.
fn fake_engine(
    dir: &Path,
    name: &str,
    commit: bool,
    stdout: &str,
    code: i32,
) -> std::path::PathBuf {
    let commit_cmd = if commit {
        "echo made > engine.txt && git add engine.txt && git commit -qm 'engine: partial work'\n"
    } else {
        ""
    };
    crate::tests::write_shim(
        dir,
        name,
        &format!("{commit_cmd}cat <<'EOF_OUT'\n{stdout}\nEOF_OUT\nexit {code}\n"),
    )
}

struct RunEnv {
    _e: std::sync::MutexGuard<'static, ()>,
    _g: std::sync::MutexGuard<'static, ()>,
}

impl RunEnv {
    fn lock() -> Self {
        let e = crate::memory::tests::env_lock()
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let g = claude_env_lock();
        for k in [
            "PHEOBE_PROVIDER",
            "PHEOBE_SANDBOX",
            "PHEOBE_CODEX_MODEL",
            "PHEOBE_OPENCODE_MODEL",
        ] {
            del_env(k);
        }
        RunEnv { _e: e, _g: g }
    }
}

impl Drop for RunEnv {
    fn drop(&mut self) {
        for k in [
            "PHEOBE_CODEX_BIN",
            "PHEOBE_OPENCODE_BIN",
            "PHEOBE_KNOWLEDGE_DIR",
        ] {
            del_env(k);
        }
    }
}

fn run(repo: &Path, provider: &str, extra: &str) -> anyhow::Result<crate::report::HandoffReport> {
    let kn = repo.parent().unwrap().join("kn");
    std::fs::create_dir_all(&kn).unwrap();
    set_env("PHEOBE_KNOWLEDGE_DIR", &kn.display().to_string());
    let t = task(&format!(
        r#"{{"task":"Write engine.txt for the parity test","done_when":{{"type":"command","run":"test -f engine.txt"}},"repo":"{}","worktree":true,"provider":"{provider}","sandbox":"free"{extra}}}"#,
        repo.display()
    ));
    crate::run::run_task(&t, None, &|_| {})
}

#[test]
fn engine_error_after_commit_is_a_failed_run_with_its_commits() {
    let _env = RunEnv::lock();
    let repo = repo();
    let bin = fake_engine(repo.parent().unwrap(), "codex", true, "", 1);
    set_env("PHEOBE_CODEX_BIN", &bin.display().to_string());
    let rep = run(&repo, "codex", "").expect("commits exist: a report, not an error");
    assert!(!rep.ok);
    assert_eq!(
        rep.commits.len(),
        1,
        "the engine's commit is in the report: {rep:#?}"
    );
    assert!(
        rep.blocked.as_deref().unwrap().contains("engine failed"),
        "{:?}",
        rep.blocked
    );
    let (_, code) = crate::report::cli_outcome("parity", Ok(rep));
    assert_eq!(
        code,
        crate::report::EXIT_NOT_OK,
        "exit 1, not 2 'never ran'"
    );
}

#[test]
fn engine_error_before_any_commit_stays_never_ran() {
    let _env = RunEnv::lock();
    let repo = repo();
    let bin = fake_engine(repo.parent().unwrap(), "codex", false, "", 1);
    set_env("PHEOBE_CODEX_BIN", &bin.display().to_string());
    let res = run(&repo, "codex", "");
    assert!(res.is_err(), "nothing produced → the pre-46 error path");
    let (rep, code) = crate::report::cli_outcome("parity", res);
    assert_eq!(code, crate::report::EXIT_ERROR);
    assert!(!rep.ok);
}

#[test]
fn the_engines_reported_cost_reaches_the_handoff_report() {
    let _env = RunEnv::lock();
    let repo = repo();
    // opencode step_finish cost: 0.05 + 0.1; the engine also commits the file
    let stream = r#"{"type":"text","timestamp":1,"sessionID":"s","part":{"id":"p1","type":"text","text":"{\"ok\":true,\"summary\":\"wrote it\"}","time":{"start":1,"end":2}}}
{"type":"step_finish","timestamp":2,"sessionID":"s","part":{"id":"p2","type":"step-finish","tokens":{"input":10,"output":5},"cost":0.05}}
{"type":"step_finish","timestamp":3,"sessionID":"s","part":{"id":"p3","type":"step-finish","tokens":{"input":20,"output":5},"cost":0.1}}"#;
    let bin = fake_engine(repo.parent().unwrap(), "opencode", true, stream, 0);
    set_env("PHEOBE_OPENCODE_BIN", &bin.display().to_string());
    let rep = run(&repo, "opencode", "").unwrap();
    assert!(rep.ok, "{rep:#?}");
    let usd = rep
        .usage
        .as_ref()
        .and_then(|u| u.usd)
        .expect("usd was hard-coded None before PHEOBE-46");
    assert!((usd - 0.15).abs() < 1e-9, "usd {usd}");
}

// ── provider: env > task > openai ────────────────────────────────────────────
#[test]
fn provider_precedence_and_validation() {
    let _env = RunEnv::lock();
    let base = r#"{"task":"x","done_when":{"type":"command","run":"true"}"#;
    assert_eq!(task(&format!("{base}}}")).effective_provider(), "openai");
    let t = task(&format!(r#"{base},"provider":"cursor"}}"#));
    assert_eq!(t.effective_provider(), "cursor");
    t.validate().unwrap();
    set_env("PHEOBE_PROVIDER", "kimi");
    assert_eq!(t.effective_provider(), "kimi", "operator env wins");
    del_env("PHEOBE_PROVIDER");
    let bad = task(&format!(r#"{base},"provider":"vim"}}"#));
    let err = format!("{:#}", bad.validate().unwrap_err());
    assert!(
        err.contains("unknown provider 'vim'") && err.contains("claude-sdk"),
        "{err}"
    );
}

// ── shared engine helpers ────────────────────────────────────────────────────
#[test]
fn moderate_confinement_makes_only_the_engines_own_state_writable() {
    let m = crate::engine::Mounts {
        home: Some("/home/u".into()),
        ..Default::default()
    };
    let a = crate::engine::moderate_bwrap_args(
        "opencode",
        &["run".into()],
        Path::new("/w"),
        &m,
        crate::engine::OPENCODE_HOME_RW,
    )
    .join(" ");
    assert!(a.contains("--ro-bind-try /home/u /home/u"), "{a}");
    assert!(a.contains("--bind-try /home/u/.local/share/opencode /home/u/.local/share/opencode"));
    assert!(
        !a.contains("/home/u/.claude"),
        "claude's state is not opencode's: {a}"
    );
    assert!(a.contains("--bind /w /w") && a.ends_with("opencode run"));
}

#[test]
fn strict_is_refused_for_network_engines_and_free_is_plain() {
    for engine in ["opencode", "kimi", "agy"] {
        let err =
            crate::engine::command(engine, "x", &[], Path::new("/w"), Some(&Tier::Strict), &[])
                .map(|_| ())
                .unwrap_err()
                .to_string();
        assert!(
            err.contains(&format!(
                "{engine} worker: sandbox 'strict' is not supported"
            )),
            "{err}"
        );
    }
    let c = crate::engine::command(
        "kimi",
        "kimi",
        &["-p".into()],
        Path::new("/w"),
        Some(&Tier::Free),
        &[],
    )
    .unwrap();
    assert_eq!(c.get_program(), "kimi");
}

#[test]
fn ttl_caps_every_engine_timeout_and_env_model_beats_task() {
    use crate::engine::{effective_timeout, resolve_model};
    assert_eq!(effective_timeout(3600, Some(Duration::from_secs(90))), 90);
    assert_eq!(effective_timeout(60, Some(Duration::from_secs(900))), 60);
    assert_eq!(effective_timeout(600, None), 600);
    assert_eq!(effective_timeout(600, Some(Duration::from_millis(10))), 1);
    assert_eq!(
        resolve_model(Some("env-m".into()), Some("task-m")).as_deref(),
        Some("env-m")
    );
    assert_eq!(
        resolve_model(Some("  ".into()), Some("task-m")).as_deref(),
        Some("task-m")
    );
    assert_eq!(resolve_model(None, None), None);
}

#[test]
fn a_timed_out_engine_is_actually_killed() {
    let _env = RunEnv::lock();
    let dir = std::env::temp_dir().join(format!("pheobe-parity-kill-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pidfile = dir.join("pid");
    let bin = crate::tests::write_shim(
        &dir,
        "codex",
        &format!("echo $$ > {}\nexec sleep 60\n", pidfile.display()),
    );
    set_env("PHEOBE_CODEX_BIN", &bin.display().to_string());
    let w = crate::worker_codex::CodexWorker::from_env().unwrap();
    let ctx = crate::worker::WorkerCtx {
        ttl: Some(Duration::from_secs(1)),
        sandbox: Some(Tier::Free),
        ..Default::default()
    };
    let started = std::time::Instant::now();
    let err = format!("{:#}", w.run_with("p", &dir, &ctx).unwrap_err());
    assert!(err.contains("timed out after 1s and was killed"), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(20),
        "codex had no timeout before PHEOBE-46"
    );
    let pid = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .to_string();
    let alive = std::path::Path::new(&format!("/proc/{pid}")).exists()
        && !std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .unwrap_or_default()
            .contains(") Z ");
    assert!(
        !alive,
        "the engine (pid {pid}) must be dead, not left running"
    );
}
