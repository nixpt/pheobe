//! PHEOBE-47: env-redirected engine config dirs are mounted in the moderate sandbox.
//! s463: agent-launch set CLAUDE_CONFIG_DIR to a common agent home under
//! /workspace/.squad/…; bwrap didn't mount it and claude-sdk came up "Not logged in".

use crate::engine::{
    moderate_bwrap_args, redirected_config, Mounts, AGY_ENV_RW, CLAUDE_ENV_RW, KIMI_ENV_RW,
    OPENCODE_ENV_RW,
};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// A scratch dir OUTSIDE /tmp (moderate mounts a private tmpfs on /tmp).
fn scratch(name: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().unwrap().join("target"));
    let d = base.join(format!("pheobe47-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn env_of(pairs: Vec<(&'static str, PathBuf)>) -> impl Fn(&str) -> Option<OsString> {
    move |k| {
        pairs
            .iter()
            .find(|(n, _)| *n == k)
            .map(|(_, v)| v.clone().into_os_string())
    }
}

#[test]
fn engine_env_lists_are_the_verified_ones() {
    assert_eq!(CLAUDE_ENV_RW, &["CLAUDE_CONFIG_DIR"]);
    assert_eq!(OPENCODE_ENV_RW, &["OPENCODE_CONFIG_DIR"]);
    assert_eq!(KIMI_ENV_RW, &["CECE_HOME", "KIMI_SHARE_DIR"]);
    assert!(AGY_ENV_RW.is_empty());
}

#[test]
fn unset_env_binds_nothing() {
    let (rw, ro, log) = redirected_config(CLAUDE_ENV_RW, &|_| None, Some(Path::new("/h")), &[]);
    assert!(rw.is_empty() && ro.is_empty() && log.is_empty());
}

#[test]
fn a_redirected_dir_is_bound_rw_and_logged() {
    let d = scratch("rw");
    let (rw, ro, log) = redirected_config(
        CLAUDE_ENV_RW,
        &env_of(vec![("CLAUDE_CONFIG_DIR", d.clone())]),
        Some(Path::new("/h")),
        &[],
    );
    assert_eq!(rw, vec![d.clone()]);
    assert!(ro.is_empty());
    assert!(
        log.iter()
            .any(|l| l.contains("binding $CLAUDE_CONFIG_DIR=") && l.contains("(rw)")),
        "{log:?}"
    );
    std::fs::remove_dir_all(&d).ok();
}

#[test]
fn nonexistent_or_relative_dirs_are_ignored() {
    let get = env_of(vec![
        ("CECE_HOME", PathBuf::from("/nonexistent/pheobe47")),
        ("KIMI_SHARE_DIR", PathBuf::from("relative/dir")),
    ]);
    let (rw, ro, log) = redirected_config(KIMI_ENV_RW, &get, Some(Path::new("/h")), &[]);
    assert!(rw.is_empty() && ro.is_empty());
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log.iter().all(|l| l.contains("not bound")));
}

#[test]
fn root_and_home_or_its_ancestors_are_refused() {
    let home = scratch("home");
    for bad in [
        PathBuf::from("/"),
        home.clone(),
        home.parent().unwrap().to_path_buf(),
    ] {
        let (rw, _, log) = redirected_config(
            CLAUDE_ENV_RW,
            &env_of(vec![("CLAUDE_CONFIG_DIR", bad.clone())]),
            Some(&home),
            &[],
        );
        assert!(rw.is_empty(), "{} must not be bound rw", bad.display());
        assert!(log[0].contains("wholesale"), "{log:?}");
    }
    std::fs::remove_dir_all(&home).ok();
}

#[test]
fn symlink_targets_outside_the_mounts_are_bound_read_only_covered_ones_are_not() {
    let d = scratch("links");
    let outside = scratch("outside");
    let covered = scratch("covered");
    std::fs::write(outside.join("token.json"), "t").unwrap();
    std::fs::write(covered.join("settings.json"), "s").unwrap();
    std::os::unix::fs::symlink(outside.join("token.json"), d.join(".credentials.json")).unwrap();
    std::os::unix::fs::symlink(covered.join("settings.json"), d.join("settings.json")).unwrap();
    std::os::unix::fs::symlink("/nonexistent/dangling", d.join("dangling")).unwrap();
    let (rw, ro, _) = redirected_config(
        CLAUDE_ENV_RW,
        &env_of(vec![("CLAUDE_CONFIG_DIR", d.clone())]),
        Some(Path::new("/h")),
        std::slice::from_ref(&covered),
    );
    assert_eq!(rw, vec![d.clone()]);
    assert_eq!(
        ro,
        vec![outside.join("token.json")],
        "only the uncovered target, never a dangling one"
    );
    for p in [d, outside, covered] {
        std::fs::remove_dir_all(p).ok();
    }
}

#[test]
fn bwrap_argv_binds_redirects_after_home() {
    let m = Mounts {
        home: Some("/h".into()),
        config_rw: vec!["/cfg/claude".into()],
        config_ro: vec!["/elsewhere/token.json".into()],
        ..Default::default()
    };
    let a = moderate_bwrap_args("claude", &["-p".into()], Path::new("/w"), &m, &[".claude"]);
    let pos = |flag: &str, p: &str| {
        a.windows(3)
            .position(|w| w[0] == flag && w[1] == p && w[2] == p)
            .unwrap_or_else(|| panic!("{flag} {p} missing in {a:?}"))
    };
    let home = pos("--ro-bind-try", "/h");
    assert!(
        pos("--bind-try", "/cfg/claude") > home,
        "redirect must come after the $HOME ro bind"
    );
    assert!(pos("--ro-bind-try", "/elsewhere/token.json") > home);
}

/// A real moderate bwrap run: a file under a redirected CLAUDE_CONFIG_DIR that lives
/// outside $HOME/.claude (and outside /tmp) is readable AND writable inside the sandbox.
#[test]
fn redirected_config_dir_is_reachable_inside_real_bwrap() {
    if !Path::new("/usr/bin/bwrap").exists() {
        eprintln!("skip: no bwrap");
        return;
    }
    let cfg = scratch("cfg");
    let wt = scratch("wt");
    std::fs::write(cfg.join("probe"), "reachable").unwrap();
    let (rw, ro, _) = redirected_config(
        CLAUDE_ENV_RW,
        &env_of(vec![("CLAUDE_CONFIG_DIR", cfg.clone())]),
        None,
        &[],
    );
    let m = Mounts {
        config_rw: rw,
        config_ro: ro,
        ..Default::default()
    };
    let script = format!(
        "cat {0}/probe && echo refreshed > {0}/token && echo ok",
        cfg.display()
    );
    let a = moderate_bwrap_args("/bin/sh", &["-c".into(), script], &wt, &m, &[]);
    let out = std::process::Command::new("/usr/bin/bwrap")
        .args(&a)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "bwrap failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        stdout.contains("reachable") && stdout.contains("ok"),
        "{stdout}"
    );
    assert_eq!(
        std::fs::read_to_string(cfg.join("token")).unwrap().trim(),
        "refreshed",
        "rw bind"
    );

    // control: without the redirect bind the same path does not exist in the sandbox
    let a = moderate_bwrap_args(
        "/bin/sh",
        &["-c".into(), format!("cat {}/probe", cfg.display())],
        &wt,
        &Mounts::default(),
        &[],
    );
    let out = std::process::Command::new("/usr/bin/bwrap")
        .args(&a)
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "control: unbound config dir must be invisible"
    );
    for p in [cfg, wt] {
        std::fs::remove_dir_all(p).ok();
    }
}
