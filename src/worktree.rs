//! Worktree isolation — pheobe never cooks in the parent's checkout.
//! Ladder: kitchen (when present) > buckets worktree > plain git worktree add.
//! Warn when the given repo looks like a primary source checkout and
//! worktree was declined.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Kitchen {
    pub worktree: PathBuf,
    pub branch: String,
    pub repo: PathBuf,
}

fn run(cmd: &mut Command) -> Result<String> {
    let out = cmd.output().context("spawn failed")?;
    if !out.status.success() {
        // git puts "nothing to commit" on STDOUT — an empty reason here is
        // what issue 09 looked like from the outside
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        let why = if err.is_empty() {
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        } else {
            err
        };
        bail!("command failed ({}): {why}", out.status)
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn is_source_checkout(repo: &Path) -> bool {
    repo.join(".git").is_dir()
}

/// Issue 03: a pre-existing `pheobe/<slug>` branch must never be checked out
/// silently (stale base). Refuse-after-suffix: `<base>-2`, `-3`, … first free
/// wins; also prune stale registrations so `git worktree add` can't fail with
/// "missing but already registered worktree" with no hint.
pub fn next_free_branch(repo: &Path, base: &str) -> Result<String> {
    let _ = run(Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "prune"]));
    for i in 1..100 {
        let candidate = if i == 1 {
            base.to_string()
        } else {
            format!("{base}-{i}")
        };
        let exists = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{candidate}"),
            ])
            .status()
            .is_ok_and(|s| s.success());
        if !exists {
            return Ok(candidate);
        }
    }
    bail!("branch '{base}' and 99 suffixed variants all exist — clean up with `kitchen list`")
}

/// The line pheobe keeps in a worktree's git exclude file.
pub const EXCLUDE_LINE: &str = "/.pheobe/";

/// Make every git command run in `wt` ignore pheobe's sidecar dir (PHEOBE-48).
///
/// pheobe's own commit already excludes `:!/.pheobe`, but an engine that commits
/// by itself (`git add -A && git commit`) used to sweep `.pheobe/plan.json` in
/// (s463: foreman-v9 57d13fe). The exclude file is resolved with
/// `git rev-parse --git-path info/exclude` from inside the worktree. For a linked
/// worktree that's the clone's shared exclude, which is clone-local and never
/// committed. The repo's tracked `.gitignore` is not touched. Idempotent.
pub fn ensure_pheobe_excluded(wt: &Path) -> Result<PathBuf> {
    let rel = run(Command::new("git").arg("-C").arg(wt).args([
        "rev-parse",
        "--git-path",
        "info/exclude",
    ]))?;
    let path = {
        let p = PathBuf::from(rel.trim());
        if p.is_absolute() {
            p
        } else {
            wt.join(p)
        }
    };
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == EXCLUDE_LINE) {
        return Ok(path);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut body = existing;
    if !body.is_empty() && !body.ends_with('\n') {
        body.push('\n');
    }
    body.push_str(EXCLUDE_LINE);
    body.push('\n');
    std::fs::write(&path, body)?;
    Ok(path)
}

/// Commits since `base` that nonetheless carry `.pheobe/` (e.g. an engine used
/// `git add -f`): returned as "<sha> <path>" for the report's doubts.
pub fn commits_touching_pheobe(wt: &Path, base: &str) -> Result<Vec<String>> {
    let out = run(Command::new("git").arg("-C").arg(wt).args([
        "log",
        "--format=%h",
        "--name-only",
        &format!("{base}..HEAD"),
        "--",
        ".pheobe",
    ]))?;
    let mut hits = Vec::new();
    let mut sha = String::new();
    for line in out.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with(".pheobe") {
            hits.push(format!("{sha} {line}"));
        } else {
            sha = line.to_string();
        }
    }
    Ok(hits)
}

/// Provision an isolated working copy. Returns (worktree path, branch).
pub fn provision(repo: &Path, branch: &str) -> Result<(PathBuf, String)> {
    let (wt, branch) = provision_raw(repo, branch)?;
    ensure_pheobe_excluded(&wt)?;
    Ok((wt, branch))
}

fn provision_raw(repo: &Path, branch: &str) -> Result<(PathBuf, String)> {
    if !is_source_checkout(repo) {
        // already a worktree — find the source repo and branch from it
        bail!("given repo is already a worktree; pass the source repo (kitchen resolves this)")
    }
    let branch = next_free_branch(repo, branch)?;
    if buckets_available() {
        return provision_buckets(repo, &branch);
    }
    // plain git fallback
    let wt = repo.join(format!(
        "../{}-{}",
        repo.file_name().and_then(|n| n.to_str()).unwrap_or("repo"),
        branch.replace('/', "-")
    ));
    run(Command::new("git").arg("-C").arg(repo).args([
        "worktree",
        "add",
        &wt.to_string_lossy(),
        "-b",
        &branch,
    ]))?;
    Ok((wt, branch))
}

fn buckets_available() -> bool {
    which("buckets")
}
fn which(bin: &str) -> bool {
    Command::new("sh")
        .args(["-c", &format!("command -v {bin} >/dev/null 2>&1")])
        .status()
        .is_ok_and(|s| s.success())
}

fn provision_buckets(repo: &Path, branch: &str) -> Result<(PathBuf, String)> {
    let wt = run(Command::new("buckets").args([
        "worktree",
        "create",
        &repo.display().to_string(),
        branch,
    ]))?;
    let last = wt
        .lines()
        .last()
        .context("buckets printed nothing")?
        .trim()
        .to_string();
    if !Path::new(&last).is_dir() {
        bail!("buckets worktree create did not produce a directory: {last}")
    }
    Ok((PathBuf::from(last), branch.to_string()))
}

/// Tear down a worktree pheobe just provisioned. Used when the model turn
/// never starts (issue 04) so empty branches do not consume the suffix
/// namespace. `--force` is required: a fresh branch with no unique commits
/// is still registered as a worktree.
pub fn teardown(repo: &Path, wt: &Path, branch: &str) -> Result<()> {
    if buckets_available() {
        run(Command::new("buckets").args([
            "worktree",
            "remove",
            &repo.display().to_string(),
            &wt.display().to_string(),
            branch,
            "--force",
        ]))?;
        return Ok(());
    }
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["worktree", "remove", "--force"])
        .arg(wt)
        .status();
    let _ = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["branch", "-D", branch])
        .status();
    Ok(())
}

/// Operator override: leave a failed-run worktree on disk for inspection.
pub fn keep_requested() -> bool {
    match std::env::var("PHEOBE_KEEP_WORKTREE") {
        Ok(v) => matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"),
        Err(_) => false,
    }
}

/// `git status --porcelain` lines, RAW — no trim. Issue 01: trimming the
/// whole output eats the leading space of the first line, and `line[3..]`
/// then yields `alc.py` for ` M calc.py`. Every real run's first status
/// entry is the edited file, so this exact mangling blocked the exit gate.
pub fn status_dirty(wt: &Path) -> Result<bool> {
    // `.pheobe/` is pheobe's own sidecar state (plan file, learning scrubs)
    // and must never count as "the model changed something" — a run whose
    // only diff is `.pheobe/plan.json` did no work and has nothing to commit.
    Ok(status_porcelain(wt)?
        .into_iter()
        .find(|line| {
            porcelain_path(line)
                .map(|p| !(p == ".pheobe" || p.starts_with(".pheobe/")))
                .unwrap_or(true)
        })
        .is_some())
}

pub fn status_porcelain(wt: &Path) -> Result<Vec<String>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(["status", "--porcelain"])
        .output()?;
    if !out.status.success() {
        bail!(
            "git status failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let txt = String::from_utf8_lossy(&out.stdout);
    Ok(txt
        .lines()
        .map(|l| l.to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Extract the path from one raw porcelain line: `XY<space>path`, rename
/// `XY<space>old -> new` → the NEW path, quoted paths dequoted.
fn porcelain_path(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    let rest = &line[3..];
    let path = if rest.contains(" -> ") {
        rest.rsplit(" -> ").next().unwrap_or(rest)
    } else {
        rest
    };
    let path = path.trim();
    let path = path
        .strip_prefix('"')
        .and_then(|p| p.strip_suffix('"'))
        .unwrap_or(path);
    if path.is_empty() {
        None
    } else {
        Some(path.to_string())
    }
}

/// Pre-commit scope check (issue-02 semantics): the allowlist is about what
/// the model *edited*. `.pheobe/` is pheobe's own state and never a
/// violation; untracked (`??`) entries are bash-run byproducts, reported
/// separately as `byproducts` — they cannot silently enter the commit
/// because `commit()` stages by pathspec, never `add -A`.
pub fn check_allowlist(wt: &Path, paths_allow: &[String]) -> Result<(Vec<String>, Vec<String>)> {
    let mut violations = vec![];
    let mut byproducts = vec![];
    for line in status_porcelain(wt)? {
        let Some(path) = porcelain_path(&line) else {
            continue;
        };
        if path == ".pheobe" || path.starts_with(".pheobe/") {
            continue;
        }
        let inside = paths_allow.iter().any(|a| {
            let a = a.trim_end_matches('/');
            path == a || path.starts_with(a)
        });
        if !inside {
            if line.starts_with("??") {
                byproducts.push(path);
            } else {
                violations.push(path);
            }
        }
    }
    Ok((violations, byproducts))
}

/// Stage for commit — by pathspec when the task has an allowlist (issue 02:
/// bash-created byproducts like `__pycache__/` must not sneak into the
/// commit), `.pheobe` excluded unconditionally.
/// Does `wt`'s git exclude carry pheobe's line (PHEOBE-48)?
///
/// Keyed on the exclude file pheobe itself writes, not `git check-ignore`:
/// check-ignore answers "not ignored" in cases where `git add` still refuses a
/// pathspec naming the path (seen in the run_task e2e test), so the two disagree.
fn pheobe_excluded(wt: &Path) -> bool {
    let Ok(rel) = run(Command::new("git").arg("-C").arg(wt).args([
        "rev-parse",
        "--git-path",
        "info/exclude",
    ])) else {
        return false;
    };
    let p = PathBuf::from(rel.trim());
    let path = if p.is_absolute() { p } else { wt.join(p) };
    std::fs::read_to_string(path)
        .map(|t| t.lines().any(|l| l.trim() == EXCLUDE_LINE))
        .unwrap_or(false)
}

fn stage(wt: &Path, paths_allow: &[String]) -> Result<()> {
    let mut args = vec!["add".to_string(), "-A".to_string(), "--".to_string()];
    if paths_allow.is_empty() {
        args.push(".".into());
    } else {
        for a in paths_allow {
            args.push(a.trim_end_matches('/').to_string());
        }
    }
    // Belt-and-braces: exclude .pheobe by pathspec only when pheobe's exclude line
    // isn't in place. Once it is, git rejects a pathspec naming the ignored path
    // ("The following paths are ignored…", exit 1), so the two guards must not stack.
    if !pheobe_excluded(wt) {
        args.push(":!/.pheobe".into());
    }
    run(Command::new("git").arg("-C").arg(wt).args(&args)).map(|_| ())
}

/// Commit with a provenance trailer (borrowed from commit-msg-agent-trailer).
/// Returns `None` when staging left nothing to commit — issue 09: a tree
/// that is "dirty" only from byproducts (`__pycache__/`) or from files the
/// engine already committed itself must not turn into a failed run.
pub fn commit(
    wt: &Path,
    task_id: &str,
    message: &str,
    paths_allow: &[String],
) -> Result<Option<String>> {
    stage(wt, paths_allow)?;
    let staged = Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(["diff", "--cached", "--quiet"])
        .status()
        .context("spawn failed")?;
    if staged.success() {
        return Ok(None);
    }
    let trailer = format!("Pheobe-Task: {task_id}");
    let hash = run(Command::new("git").arg("-C").arg(wt).args([
        "-c",
        "user.name=pheobe",
        "-c",
        "user.email=pheobe@local",
        "commit",
        "-m",
        message,
        "-m",
        &trailer,
        "--no-verify",
        "--quiet",
    ]))?;
    let _ = hash;
    let sha = run(Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(["rev-parse", "--short", "HEAD"]))?;
    Ok(Some(sha))
}

/// Full sha of HEAD — recorded right after provision so the report can
/// list everything that landed since, whoever ran `git commit`.
pub fn head_sha(wt: &Path) -> Result<String> {
    run(Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(["rev-parse", "HEAD"]))
}

/// Short shas of every commit on the branch since `base`, oldest first.
/// Issue 05: the engine (claude/opencode/codex…) commits itself when it
/// follows the persona's COMMIT stage, so the report cannot be built from
/// pheobe's own commit gate alone.
pub fn commits_since(wt: &Path, base: &str) -> Result<Vec<String>> {
    let out = run(Command::new("git").arg("-C").arg(wt).args([
        "log",
        "--reverse",
        "--format=%h",
        &format!("{base}..HEAD"),
    ]))?;
    Ok(out
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect())
}

/// Ship: push the branch (never a protected one). Merge is the parent's job.
pub fn push(wt: &Path, branch: &str) -> Result<()> {
    match branch {
        "main" | "master" | "dev" => bail!("refusing to ship protected branch"),
        _ => {}
    }
    run(Command::new("git")
        .arg("-C")
        .arg(wt)
        .args(["push", "-u", "origin", branch]))?;
    Ok(())
}
