//! PHEOBE-34: CLI contract tests. Spawn the built binary
//! (`env!("CARGO_BIN_EXE_pheobe")`) with a scratch HOME / knowledge dir so
//! nothing touches `~/.pheobe`. Cargo `[[test]]` target — CARGO_BIN_EXE is
//! not set inside the lib's `#[cfg(test)]` modules.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

static SEQ: AtomicU32 = AtomicU32::new(0);

struct Fx {
    home: PathBuf,
    know: PathBuf,
    repo: PathBuf,
}

impl Fx {
    fn new() -> Self {
        let n = SEQ.fetch_add(1, Ordering::SeqCst);
        let root = std::env::temp_dir().join(format!("pheobe-cli-{}-{}", std::process::id(), n));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let home = root.join("home");
        let know = root.join("knowledge");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&know).unwrap();
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["config", "user.email", "t@t"]);
        git(&repo, &["config", "user.name", "t"]);
        std::fs::write(repo.join("a.txt"), "ok\n").unwrap();
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname=\"x\"\nversion=\"0.1.0\"\n",
        )
        .unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-q", "-m", "init"]);
        Self { home, know, repo }
    }

    fn pheobe(&self, args: &[&str]) -> Output {
        self.pheobe_env(args, &[])
    }

    fn pheobe_env(&self, args: &[&str], extra: &[(&str, &str)]) -> Output {
        self.spawn(args, extra, None)
    }

    fn pheobe_in(&self, cwd: &Path, args: &[&str]) -> Output {
        self.spawn(args, &[], Some(cwd))
    }

    fn spawn(&self, args: &[&str], extra: &[(&str, &str)], cwd: Option<&Path>) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_pheobe"));
        cmd.args(args)
            .env("HOME", &self.home)
            .env("PHEOBE_KNOWLEDGE_DIR", &self.know)
            .env("PHEOBE_MEMORY", "none")
            .env("PHEOBE_SANDBOX", "free")
            .env_remove("PHEOBE_BASE_URL")
            .env_remove("PHEOBE_MODEL")
            .env_remove("PHEOBE_PROVIDER")
            .env_remove("PHEOBE_KEEP_WORKTREE")
            .env_remove("NO_PROXY")
            .env_remove("no_proxy");
        for (k, v) in extra {
            cmd.env(k, v);
        }
        if let Some(cwd) = cwd {
            cmd.current_dir(cwd);
        }
        cmd.output().expect("spawn pheobe")
    }

    fn write_task(&self, name: &str, body: &str) -> PathBuf {
        let p = self.home.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }
}

fn git(repo: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

fn code(o: &Output) -> i32 {
    o.status.code().unwrap_or(-1)
}

fn extra_worktrees(repo: &Path) -> Vec<String> {
    let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    git(&repo, &["worktree", "list", "--porcelain"])
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

fn json_string(s: &str, key: &str) -> String {
    let pat = format!("\"{key}\": \"");
    let rest = s
        .split_once(&pat)
        .unwrap_or_else(|| panic!("no {key} in {s}"))
        .1;
    rest.split('"').next().unwrap().to_string()
}

fn dead_proxy() -> [(&'static str, &'static str); 5] {
    [
        ("HTTPS_PROXY", "http://127.0.0.1:9"),
        ("HTTP_PROXY", "http://127.0.0.1:9"),
        ("https_proxy", "http://127.0.0.1:9"),
        ("http_proxy", "http://127.0.0.1:9"),
        ("ALL_PROXY", "http://127.0.0.1:9"),
    ]
}

fn task_json(fx: &Fx, extra: &str) -> String {
    format!(
        r#"{{"task":"Add a CLI contract assertion for pheobe verify","done_when":{{"type":"command","run":"test -f a.txt"}},"paths_allow":["a.txt"],"repo":"{}","worktree":true{extra}}}"#,
        fx.repo.display()
    )
}

#[test]
fn verify_pass_and_fail_print_the_issue08_reason() {
    let fx = Fx::new();
    let pass = fx.write_task("pass.json", &task_json(&fx, ""));
    let o = fx.pheobe(&[
        "verify",
        pass.to_str().unwrap(),
        "--worktree",
        fx.repo.to_str().unwrap(),
    ]);
    assert_eq!(code(&o), 0, "pass stderr={}", stderr(&o));
    assert!(
        stdout(&o).contains("✅ verify passed:"),
        "got {}",
        stdout(&o)
    );

    let fail = fx.write_task(
        "fail.json",
        &task_json(&fx, "").replace("test -f a.txt", "false"),
    );
    let o = fx.pheobe(&[
        "verify",
        fail.to_str().unwrap(),
        "--worktree",
        fx.repo.to_str().unwrap(),
    ]);
    assert_eq!(code(&o), 1, "fail should be 1, stdout={}", stdout(&o));
    assert!(
        stderr(&o).contains("✗ done_when failed:"),
        "issue 08 contract, got {}",
        stderr(&o)
    );
}

#[test]
fn host_setup_then_finish_roundtrip() {
    let fx = Fx::new();
    let task = fx.write_task("host.json", &task_json(&fx, ""));
    let branch = format!("pheobe/p34-{}", std::process::id());
    let o = fx.pheobe(&["host", "setup", task.to_str().unwrap(), "--branch", &branch]);
    assert_eq!(code(&o), 0, "setup {}", stderr(&o));
    let out = stdout(&o);
    let wt = json_string(&out, "worktree");
    let br = json_string(&out, "branch");
    assert!(Path::new(&wt).is_dir(), "{wt}");
    let o = fx.pheobe(&["host", "finish", task.to_str().unwrap(), "--worktree", &wt]);
    assert_eq!(code(&o), 0, "finish {}", stderr(&o));
    assert!(stdout(&o).contains("\"ok\": true"), "{}", stdout(&o));
    let _ = git(&fx.repo, &["worktree", "remove", "--force", &wt]);
    let _ = git(&fx.repo, &["branch", "-D", &br]);
    assert!(
        extra_worktrees(&fx.repo).is_empty(),
        "{:?}",
        extra_worktrees(&fx.repo)
    );
}

#[test]
fn adopt_prints_every_kit_and_rejects_unknown() {
    let fx = Fx::new();
    let kits: &[(&[&str], &str)] = &[
        (&["adopt"], "pheobe adoption kits"),
        (&["adopt", "claude"], "Claude Code subagent"),
        (&["adopt", "claude-sdk"], "Claude Agent SDK"),
        (&["adopt", "opencode-self"], "opencode agent (self mode)"),
        (&["adopt", "opencode"], "opencode agent (host mode)"),
        (&["adopt", "codex"], "codex adoption"),
        (&["adopt", "cursor"], "Cursor adoption kit"),
        (&["adopt", "kimi"], "Kimi Code adoption"),
        (&["adopt", "agy"], "Antigravity (AGY)"),
    ];
    for (args, needle) in kits {
        let o = fx.pheobe(args);
        assert_eq!(code(&o), 0, "{args:?} {}", stderr(&o));
        assert!(
            stdout(&o).contains(needle),
            "{args:?} missing {needle:?} in {}",
            stdout(&o)
        );
    }
    let self_kit = fx.pheobe(&["adopt", "opencode-self"]);
    assert!(
        stdout(&self_kit).contains("opencode.jsonc snippet"),
        "opencode-self also prints the jsonc snippet"
    );
    let o = fx.pheobe(&["adopt", "nope"]);
    assert_ne!(code(&o), 0, "unknown kit must fail");
}

#[test]
fn ctx_seed_list_brief_rust_body() {
    let fx = Fx::new();
    let o = fx.pheobe(&["ctx", "seed"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let first = stdout(&o);
    assert!(first.contains("written"), "{first}");
    assert!(first.contains("kept"), "{first}");
    let o = fx.pheobe(&["ctx", "seed"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let second = stdout(&o);
    assert!(second.contains("0 written"), "{second}");
    assert!(second.contains("--force to overwrite"), "{second}");

    let o = fx.pheobe(&["ctx", "list"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("rust"), "{}", stdout(&o));

    let o = fx.pheobe(&["ctx", "brief", "--for-repo", fx.repo.to_str().unwrap()]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let brief = stdout(&o);
    assert!(
        brief.contains("The tooling passport for Rust"),
        "rust body missing: {brief}"
    );
}

#[test]
fn doctor_dead_proxy_prints_unreachable_and_exits_0() {
    let fx = Fx::new();
    let proxy = dead_proxy();
    let o = fx.pheobe_env(&["doctor"], &proxy);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(stdout(&o).contains("channel unreachable"), "{}", stdout(&o));
}

#[test]
fn update_check_offline_exits_nonzero_with_reason() {
    let fx = Fx::new();
    let proxy = dead_proxy();
    let o = fx.pheobe_env(&["update", "--check"], &proxy);
    assert_ne!(code(&o), 0, "offline --check must not look current");
    let combined = format!("{}{}", stdout(&o), stderr(&o));
    assert!(
        combined.contains("channel unreachable"),
        "reason missing: {combined}"
    );
}

#[test]
fn run_without_done_when_is_refused_and_json_flag_is_accepted() {
    let fx = Fx::new();
    let task = fx.write_task(
        "nodone.json",
        &format!(
            r#"{{"task":"Add a --json flag to the CLI","repo":"{}","worktree":true}}"#,
            fx.repo.display()
        ),
    );
    let o = fx.pheobe(&["run", task.to_str().unwrap(), "--json"]);
    assert_eq!(code(&o), 1, "must refuse; stdout={}", stdout(&o));
    assert!(
        stderr(&o).contains("done_when"),
        "clear intake reason, got {}",
        stderr(&o)
    );
    assert!(
        extra_worktrees(&fx.repo).is_empty(),
        "intake refusal must not provision: {:?}",
        extra_worktrees(&fx.repo)
    );
}

#[test]
fn learn_nudge_and_check_list_smoke() {
    let fx = Fx::new();
    let repo = fx.repo.to_str().unwrap();
    let mem = [("PHEOBE_MEMORY", "local")];
    let o = fx.pheobe_env(
        &[
            "learn",
            "nudge",
            "--repo",
            repo,
            "never commit secrets in the kitchen",
        ],
        &mem,
    );
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    let o = fx.pheobe_env(&["learn", "nudges", "--repo", repo], &mem);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stdout(&o).contains("never commit secrets in the kitchen"),
        "{}",
        stdout(&o)
    );
    let o = fx.pheobe_in(&fx.repo, &["check", "list"]);
    assert_eq!(code(&o), 0, "{}", stderr(&o));
    assert!(
        stdout(&o).contains("no checkpoints") || stdout(&o).contains("NAME"),
        "{}",
        stdout(&o)
    );
}
