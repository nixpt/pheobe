//! PHEOBE-41..44: runner gaps closed for tiered dispatch (foreman FMN-4).
//! Pure functions only here; fake-binary tests live in claude.rs, exit
//! codes against the built binary in cli.rs.

use crate::report::{cli_outcome, HandoffReport, EXIT_ERROR, EXIT_NOT_OK, EXIT_OK};
use crate::sandbox::Tier;
use crate::worker_claude::{
    claude_args, effective_timeout, moderate_bwrap_args, resolve_model, Mounts,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

// ── PHEOBE-41: model knob ────────────────────────────────────────────────────
#[test]
fn model_is_appended_never_replacing_flags() {
    let a = claude_args("p", "--dangerously-skip-permissions", Some("sonnet"));
    assert_eq!(
        a,
        [
            "-p",
            "p",
            "--output-format",
            "json",
            "--dangerously-skip-permissions",
            "--model",
            "sonnet"
        ]
    );
    assert!(!claude_args("p", "--x", None).contains(&"--model".to_string()));
    assert!(!claude_args("p", "--x", Some("  ")).contains(&"--model".to_string()));
}

#[test]
fn env_model_wins_over_task_model() {
    assert_eq!(
        resolve_model(Some("opus".into()), Some("haiku")).as_deref(),
        Some("opus")
    );
    assert_eq!(resolve_model(None, Some("haiku")).as_deref(), Some("haiku"));
    assert_eq!(
        resolve_model(Some(" ".into()), Some("haiku")).as_deref(),
        Some("haiku")
    );
    assert_eq!(resolve_model(None, None), None);
}

#[test]
fn task_model_field_parses_and_unknown_fields_still_rejected() {
    let t: crate::task::Task = serde_json::from_str(
        r#"{"task":"add x","done_when":{"type":"command","run":"true"},"model":"sonnet"}"#,
    )
    .unwrap();
    assert_eq!(t.model.as_deref(), Some("sonnet"));
    assert!(serde_json::from_str::<crate::task::Task>(
        r#"{"task":"add x","done_when":{"type":"command","run":"true"},"modle":"x"}"#
    )
    .is_err());
}

// ── PHEOBE-42: exit codes ────────────────────────────────────────────────────
#[test]
fn cli_outcome_codes() {
    let mut ok = HandoffReport::failure("t", "x");
    ok.ok = true;
    ok.blocked = None;
    assert_eq!(cli_outcome("t", Ok(ok)).1, EXIT_OK);
    assert_eq!(
        cli_outcome("t", Ok(HandoffReport::failure("t", "gate"))).1,
        EXIT_NOT_OK
    );
    let (rep, code) = cli_outcome("t", Err(anyhow::anyhow!("no repo given")));
    assert_eq!(code, EXIT_ERROR);
    assert!(!rep.ok && rep.blocked.unwrap().contains("no repo given"));
}

// ── PHEOBE-43: timeout + sandbox argv ────────────────────────────────────────
#[test]
fn timeout_is_min_of_env_and_ttl() {
    assert_eq!(effective_timeout(3600, None), 3600);
    assert_eq!(effective_timeout(3600, Some(Duration::from_secs(600))), 600);
    assert_eq!(effective_timeout(30, Some(Duration::from_secs(600))), 30);
    assert_eq!(
        effective_timeout(3600, Some(Duration::from_millis(200))),
        1,
        "never 0"
    );
}

fn pos(a: &[String], xs: &[&str]) -> usize {
    a.windows(xs.len())
        .position(|w| w.iter().map(String::as_str).eq(xs.iter().copied()))
        .unwrap_or(usize::MAX)
}

#[test]
fn moderate_bwrap_argv_shape() {
    let m = Mounts {
        home: Some(PathBuf::from("/h")),
        git_dir: Some(PathBuf::from("/r/.git/worktrees/w")),
        git_common: Some(PathBuf::from("/r/.git")),
        repo_parent: Some(PathBuf::from("/p")),
        cargo_target: Some(PathBuf::from("/build/t")),
        ..Default::default()
    };
    let a = moderate_bwrap_args(
        "/opt/fake/claude",
        &["-p".into(), "x".into()],
        Path::new("/tmp/wt"),
        &m,
    );
    assert!(
        !a.contains(&"--unshare-net".to_string()),
        "moderate keeps the network for the API"
    );
    let tmpfs = pos(&a, &["--tmpfs", "/tmp"]);
    let wt = pos(&a, &["--bind", "/tmp/wt", "/tmp/wt"]);
    assert!(
        tmpfs < wt,
        "tmpfs /tmp must come before the worktree bind or it shadows it: {a:?}"
    );
    for (flag, p) in [
        ("--ro-bind-try", "/h"),
        ("--bind-try", "/h/.claude"),
        ("--bind-try", "/r/.git"),
        ("--ro-bind-try", "/p"),
        ("--bind-try", "/r/.git/worktrees/w"),
        ("--bind-try", "/build/t"),
        ("--ro-bind-try", "/opt/fake"),
    ] {
        assert!(
            pos(&a, &[flag, p, p]) != usize::MAX,
            "missing {flag} {p}: {a:?}"
        );
    }
    assert!(
        pos(&a, &["--ro-bind-try", "/h", "/h"])
            < pos(&a, &["--bind-try", "/h/.claude", "/h/.claude"])
    );
    assert_eq!(&a[a.len() - 3..], ["/opt/fake/claude", "-p", "x"]);
}

#[test]
fn strict_is_refused_for_the_claude_worker_before_spawning() {
    use crate::worker::{Worker, WorkerCtx};
    let ctx = WorkerCtx {
        sandbox: Some(Tier::Strict),
        ..Default::default()
    };
    let err = crate::worker_claude::ClaudeWorker
        .run_with("p", Path::new("/nonexistent"), &ctx)
        .unwrap_err()
        .to_string();
    assert!(err.contains("strict") && err.contains("network"), "{err}");
}

// ── PHEOBE-44: files_exist ───────────────────────────────────────────────────
#[test]
fn files_exist_parses_validates_and_verifies() {
    let dir = std::env::temp_dir().join(format!("pheobe-fe-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/a.rs"), "").unwrap();
    let t = crate::task::load_from_str(
        r#"{"task":"add src/a.rs","done_when":{"type":"files_exist","paths":["src/a.rs"]}}"#,
    )
    .unwrap();
    assert!(crate::verify::run_done_when(&t, &dir).unwrap().passed);
    let t2 = crate::task::load_from_str(
        r#"{"task":"add src/b.rs","done_when":{"type":"files_exist","paths":["src/a.rs","src/b.rs"]}}"#,
    )
    .unwrap();
    let ev = crate::verify::run_done_when(&t2, &dir).unwrap();
    assert!(!ev.passed && ev.output_excerpt.unwrap().contains("src/b.rs"));
    assert!(crate::task::load_from_str(
        r#"{"task":"add x","done_when":{"type":"files_exist","paths":[]}}"#
    )
    .is_err());
    std::fs::remove_dir_all(&dir).ok();
}
