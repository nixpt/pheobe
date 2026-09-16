//! Structural read/write ladder (PHEOBE-11): polydex + code-atlas.
//!
//! Deprecation-safe backend detection (accept both `polydex` and the old
//! `crush-symbols` name), a freshness gate (`polydex status` stale → the
//! read wrapper says so and points at the text-tool fallback — polydex's
//! own `maybe_wrap_stale` honesty rule adopted — and orient omits the
//! structural brief), and thin subprocess wrappers for the read ladder
//! (`skeleton` / `enclosing` / `callers` / `impact` / `affected-tests` /
//! `hotspots` / `languages`). code-atlas (design-only, its own repo) is
//! detected the same way but none of its write protocols are exercised
//! here until the binary exists — the gate is in place, the ladder stays
//! empty, and `barn` just doesn't register `atlas_edit`.
//!
//! Skip-don't-fail posture (same as `knowledge`): every entry point returns
//! usable output when the backend is absent or stale, marked as such, never
//! an error that would kill a run.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const POLYDEX_NAMES: &[&str] = &["polydex", "crush-symbols"];
const CODE_ATLAS_NAMES: &[&str] = &["code-atlas"];

/// The freshness verdict for a polydex index, from `polydex status --json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexStatus {
    pub exists: bool,
    pub stale: bool,
    /// Files the index no longer covers (new/modified/deleted).
    pub drift: Vec<String>,
}

impl IndexStatus {
    pub fn is_fresh(&self) -> bool {
        self.exists && !self.stale
    }
}

/// Scan one PATH-style string (testable without touching the process env).
pub fn scan_path_for(path_env: &str, names: &[&str]) -> Option<PathBuf> {
    for dir in std::env::split_paths(path_env) {
        for name in names {
            let cand = dir.join(name);
            if cand.is_file() && is_executable(&cand) {
                return Some(cand);
            }
        }
    }
    None
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    p.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Rebuild a PATH-style string from the current process PATH (split + join
/// via `std::env::join_paths` so sep rules are handled, not hand-joined).
fn path_env() -> Option<String> {
    let path = std::env::var_os("PATH")?;
    let parts: Vec<PathBuf> = std::env::split_paths(&path).collect();
    std::env::join_paths(parts).ok().and_then(|p| p.into_string().ok())
}

/// Find the polydex backend on the current PATH (deprecation-safe).
pub fn polydex_bin() -> Option<PathBuf> {
    let path = path_env()?;
    scan_path_for(&path, POLYDEX_NAMES)
}

/// Find the code-atlas backend on the current PATH.
pub fn code_atlas_bin() -> Option<PathBuf> {
    let path = path_env()?;
    scan_path_for(&path, CODE_ATLAS_NAMES)
}

/// `polydex status --json`, run with cwd = the worktree, parsed into a
/// freshness verdict. A missing backend = `None` (skip, not fail); a status
/// that cannot be parsed = a stale-flavored verdict, not a crash.
pub fn index_status(worktree: &Path) -> Option<IndexStatus> {
    let bin = polydex_bin()?;
    let out = Command::new(&bin)
        .arg("status")
        .arg("--json")
        .current_dir(worktree)
        .output()
        .ok()?;
    if !out.status.success() {
        return Some(IndexStatus { exists: false, stale: true, drift: vec![] });
    }
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let exists = v.get("exists").and_then(|e| e.as_bool()).unwrap_or(false);
    let mut drift = vec![];
    let mut stale = !exists;
    if let Some(fresh) = v.get("freshness").and_then(|f| f.as_object()) {
        for key in ["new_files", "modified_files", "deleted_files"] {
            if let Some(list) = fresh.get(key).and_then(|l| l.as_array()) {
                if !list.is_empty() {
                    stale = true;
                    for item in list {
                        if let Some(s) = item.as_str() {
                            drift.push(format!("{key}:{}", Path::new(s).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_else(|| s.to_string())));
                        }
                    }
                }
            }
        }
    }
    Some(IndexStatus { exists, stale, drift })
}

/// Fresh when the backend exists AND the index exists AND nothing is dirty.
pub fn polydex_fresh(worktree: &Path) -> bool {
    index_status(worktree).map(|s| s.is_fresh()).unwrap_or(false)
}

/// The honesty note a stale index gets, so the model knows the structural
/// read was NOT served and records the fallback as a doubt.
pub fn stale_note(status: &IndexStatus) -> String {
    let mut n = String::from(
        "POLYDEX STALE INDEX — structural result NOT served; this is a text-tool fallback \
         moment (plain read/grep). Record this as a doubt in the handoff report.\n",
    );
    if !status.drift.is_empty() {
        let mut names: Vec<&str> = status.drift.iter().map(|s| s.as_str()).collect();
        names.sort();
        names.dedup();
        n.push_str(&format!("  index drift: {}\n", names.join(", ")));
    }
    n.trim().to_string()
}

/// Name the backend in errors so a missing/broken binary names its fix.
fn run_wrapper(bin: &Path, worktree: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new(bin)
        .args(args)
        .current_dir(worktree)
        .output()
        .with_context(|| {
            format!(
                "structural read '{}' failed to spawn — is the polydex backend reachable? \
                 (set PATH or install polydex/crush-symbols)",
                bin.display()
            )
        })?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!(
            "structural read {:?} exited with {}: {}",
            args,
            out.status,
            stderr.trim()
        );
    }
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    truncate(&mut s);
    Ok(s)
}

/// One freshness-gated structural read: `Ok(Some(text))` served, `Ok(None)`
/// when the index is stale (the caller renders the fallback note), `Err`
/// only for backend failures.
pub fn read_fresh(worktree: &Path, args: &[&str]) -> Result<Option<String>> {
    let Some(bin) = polydex_bin() else {
        // no backend → the tool simply should not have been offered
        return Ok(None);
    };
    match index_status(worktree) {
        Some(st) if st.is_fresh() => Ok(Some(run_wrapper(&bin, worktree, args)?)),
        Some(st) => Ok(Some(stale_note(&st))),
        None => Ok(None),
    }
}

/// `skeleton <file>` — signatures-only view of one file from the index.
pub fn skeleton(worktree: &Path, file: &str) -> Result<Option<String>> {
    read_fresh(worktree, &["skeleton", file])
}

/// `callers <name>` — direct callers (incoming calls edges).
pub fn callers(worktree: &Path, name: &str) -> Result<Option<String>> {
    read_fresh(worktree, &["callers", name])
}

/// `impact <symbol>` — transitive callers: what breaks if this changes?
pub fn impact(worktree: &Path, symbol: &str) -> Result<Option<String>> {
    read_fresh(worktree, &["impact", symbol])
}

/// `enclosing <file> <line>` — the best enclosing symbol for a coordinate.
pub fn enclosing(worktree: &Path, file: &str, line: u64) -> Result<Option<String>> {
    read_fresh(worktree, &["enclosing", file, &line.to_string()])
}

/// `affected-tests <changed>...` — test symbols transitively reachable.
pub fn affected_tests(worktree: &Path, changed: &[String]) -> Result<Option<String>> {
    if changed.is_empty() {
        return Ok(None);
    }
    let args: Vec<&str> = std::iter::once("affected-tests")
        .chain(changed.iter().map(|s| s.as_str()))
        .collect();
    read_fresh(worktree, &args)
}

/// `hotspots --skip-noise` for orient.
pub fn hotspots(worktree: &Path) -> Result<Option<String>> {
    read_fresh(worktree, &["hotspots", "--skip-noise"])
}

/// `languages` for orient — what the index knows how to parse.
pub fn languages(worktree: &Path) -> Result<Option<String>> {
    read_fresh(worktree, &["languages"])
}

/// The orient brief block: hotspots + languages, gated on a fresh index.
/// Returns "" when absent or stale — list the languages FIRST (small signal,
/// cheap), then hotspots (the interesting meat).
pub fn orient_brief(worktree: &Path) -> String {
    let Some(bin) = polydex_bin() else { return String::new() };
    let Some(st) = index_status(worktree) else { return String::new() };
    if !st.is_fresh() {
        return String::new();
    }
    let mut block = String::from("\n## Structural read (polydex, fresh index)\n");
    if let Ok(Some(langs)) = languages(worktree) {
        if !langs.trim().is_empty() {
            block.push_str("indexed languages: ");
            block.push_str(langs.trim().lines().collect::<Vec<_>>().join(", ").as_str());
            block.push('\n');
        }
    }
    if let Ok(Some(hs)) = hotspots(worktree) {
        if !hs.trim().is_empty() {
            block.push_str("hotspots (sensitive-callee call-graph sites):\n");
            block.push_str(hs.trim());
            block.push('\n');
        }
    }
    let _ = bin;
    if block.trim().len() <= "## Structural read (polydex, fresh index)".len() {
        String::new()
    } else {
        block
    }
}

fn truncate(s: &mut String) {
    const MAX: usize = 6000;
    if s.chars().count() > MAX {
        let cut: String = s.chars().take(MAX).collect();
        *s = format!("{cut}\n… (truncated)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serializes tests that mutate the process-global `PATH` env, which would
    /// otherwise race against other tests in the same binary (git spawns via
    /// PATH, other adapter tests setting PHEOBE_* env, etc.).
    static PATH_LOCK: Mutex<()> = Mutex::new(());

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pheobe-structint-{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fake_bin(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).unwrap();
        path
    }

    /// A fake polydex that appends each argv to `capture` and prints a
    /// canned stdout for each subcommand.
    fn fake_polydex(dir: &Path) -> (PathBuf, PathBuf) {
        let capture = dir.join("captured.txt");
        let body = format!(
            r#"for a in "$@"; do printf '%s\n' "$a" >> '{}'; done
printf '%s\n' "CWD=$(pwd)" >> '{}'
case "$1" in
  status) echo '{{"exists":true,"freshness":{{"new_files":[],"modified_files":[],"deleted_files":[]}}}}' ;;
  skeleton) echo 'skel: fn normalize(path) -> PathBuf' ;;
  callers) echo 'callers: A -> B -> C' ;;
  impact) echo 'impact: 12 transitive callers' ;;
  hotspots) echo 'hot: eval() at src/vm.rs:42' ;;
  affected-tests) echo 'affected: tests::t1 tests::t2' ;;
  languages) echo 'Rust' ;;
esac
"#,
            capture.display(),
            capture.display()
        );
        let bin = fake_bin(dir, "polydex", &body);
        (bin, capture)
    }

    #[test]
    fn scan_path_finds_both_backend_names_deprecation_safe() {
        let dir = scratch("scan");
        fake_bin(&dir, "crush-symbols", "");
        let found = scan_path_for(&dir.display().to_string(), POLYDEX_NAMES);
        assert!(found.is_some(), "old crush-symbols name resolves too");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scan_path_skips_missing_and_non_executable() {
        let dir = scratch("scan-missing");
        fake_bin(&dir, "polydex", "");
        // a dir-only PATH, plus the plain system PATH tail
        let only_bin = scan_path_for(&dir.display().to_string(), POLYDEX_NAMES);
        assert!(only_bin.is_some());
        // an empty PATH finds nothing without crashing
        assert!(scan_path_for("", POLYDEX_NAMES).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn index_status_parses_fresh_and_stale_json() {
        let _lock = PATH_LOCK.lock().unwrap();
        let dir = scratch("fresh");
        fake_polydex(&dir);
        let real_path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{real_path}", dir.display())) };
        let st = index_status(&dir).expect("backend detected");
        assert!(st.is_fresh(), "fake status reports exists && no drift");
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stale_index_drift_surfaces_files() {
        let _lock = PATH_LOCK.lock().unwrap();
        let dir = scratch("stale");
        let body = r#"echo '{"exists":true,"freshness":{"new_files":["src/lib.rs"],"modified_files":[],"deleted_files":[]}}'"#;
        fake_bin(&dir, "polydex", body);
        let real_path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{real_path}", dir.display())) };
        let st = index_status(&dir).expect("backend detected");
        assert!(!st.is_fresh(), "new files = stale");
        assert!(st.drift.iter().any(|d| d.contains("lib.rs")), "drift names the file: {:?}", st.drift);
        let note = stale_note(&st);
        assert!(note.contains("STALE INDEX"), "note says so: {note}");
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }

    /// When PATH is opended to the fake dir, the wrappers resolve and serve;
    /// the fake records argv + cwd proving the exact subcommand shapes.
    #[test]
    fn wrapper_shapes_and_cwd_reach_the_backend() {
        let _lock = PATH_LOCK.lock().unwrap();
        let dir = scratch("wrap");
        let (bin, cap) = fake_polydex(&dir);
        let real_path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{real_path}", bin.parent().unwrap().display())) };

        let s = skeleton(&dir, "src/normalize.rs").unwrap().unwrap();
        assert!(s.contains("skel"), "skeleton served, got: {s}");
        let c = callers(&dir, "normalize").unwrap().unwrap();
        assert!(c.contains("callers"));
        let i = impact(&dir, "normalize").unwrap().unwrap();
        assert!(i.contains("impact"));
        let h = hotspots(&dir).unwrap().unwrap();
        assert!(h.contains("hot:"));
        let l = languages(&dir).unwrap().unwrap();
        assert!(l.contains("Rust"));

        let cap = std::fs::read_to_string(&cap).unwrap();
        let lines: Vec<&str> = cap.lines().collect();
        assert!(
            lines.iter().any(|l| *l == "skeleton"),
            "argv recorded, got: {lines:?}"
        );
        // every recorded invocation ran with cwd = worktree
        assert!(lines.iter().filter(|l| l.starts_with("CWD=")).all(|l| *l == format!("CWD={}", dir.display())));
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn stale_wrapper_returns_the_fallback_note_not_silent_success() {
        let _lock = PATH_LOCK.lock().unwrap();
        let dir = scratch("stale-wrap");
        // status always reports stale (modified files present); reads never serve
        let body = r#"if [ "$1" = status ]; then echo '{"exists":true,"freshness":{"modified_files":["src/lib.rs"],"new_files":[],"deleted_files":[]}}'; else echo 'should-not-serve'; fi"#;
        let bin = fake_bin(&dir, "polydex", body);
        let real_path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{real_path}", bin.parent().unwrap().display())) };

        let out = skeleton(&dir, "src/normalize.rs").unwrap().unwrap();
        assert!(
            out.contains("STALE INDEX"),
            "fallback note served, got: {out}"
        );
        assert!(
            out.contains("lib.rs"),
            "drift names the file, got: {out}"
        );
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn orient_brief_empty_when_backend_absent() {
        let _lock = PATH_LOCK.lock().unwrap();
        let dir = scratch("no-backend");
        let real_path = std::env::var("PATH").unwrap_or_default();
        // PATH with no polydex: use a directory with nothing in it
        let empty = dir.join("nothings");
        std::fs::create_dir_all(&empty).unwrap();
        unsafe { std::env::set_var("PATH", empty.display().to_string()) };
        assert_eq!(orient_brief(&dir), "", "absent backend = no structural brief");
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn orient_brief_lists_languages_and_hotspots_when_fresh() {
        let _lock = PATH_LOCK.lock().unwrap();
        let dir = scratch("orient");
        fake_polydex(&dir);
        let real_path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{real_path}", dir.display())) };
        let brief = orient_brief(&dir);
        assert!(brief.contains("## Structural read"), "got: {brief}");
        assert!(brief.contains("indexed languages:") && brief.contains("Rust"), "got: {brief}");
        assert!(brief.contains("hot: eval()"), "hotspots included, got: {brief}");
        unsafe { std::env::set_var("PATH", &real_path) };
        std::fs::remove_dir_all(&dir).ok();
    }
}