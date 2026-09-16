//! PHEOBE-35: Structural tool ladder tests (`tools/structural.rs`).
//! Tests optional sym_* and atlas_edit tools with fake backends on PATH.

use crate::structint::tests::PATH_LOCK;
use crate::task::Task;
use crate::tools::{barn, dispatch, schemas, ToolCtx};
use serde_json::json;
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pheobe-struct-test-{name}-{}-{}",
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
    let mut perms = std::fs::metadata(&path).unwrap().permissions();
    #[allow(clippy::permissions_set_readonly_false)]
    {
        use std::os::unix::fs::PermissionsExt;
        perms.set_mode(0o755);
    }
    std::fs::set_permissions(&path, perms).unwrap();
    path
}

fn test_task() -> Task {
    serde_json::from_str(r#"{"task":"test","done_when":{"type":"command","run":"true"}}"#).unwrap()
}

fn test_ctx<'a>(wt: &'a Path, task: &'a Task) -> ToolCtx<'a> {
    ToolCtx {
        wt,
        task,
        task_id: "test-task-structural",
    }
}

struct PathGuard {
    _lock: std::sync::MutexGuard<'static, ()>,
    orig_path: String,
}

impl PathGuard {
    fn with_dir(dir: &Path) -> Self {
        let lock = PATH_LOCK.lock().unwrap();
        let orig_path = std::env::var("PATH").unwrap_or_default();
        unsafe { std::env::set_var("PATH", format!("{}:{orig_path}", dir.display())) };
        Self {
            _lock: lock,
            orig_path,
        }
    }

    fn without_backends() -> Self {
        let lock = PATH_LOCK.lock().unwrap();
        let orig_path = std::env::var("PATH").unwrap_or_default();
        let names = &["polydex", "crush-symbols", "code-atlas"];
        let filtered: Vec<PathBuf> = std::env::split_paths(&orig_path)
            .filter(|dir| !names.iter().any(|name| dir.join(name).is_file()))
            .collect();
        let filtered_path = std::env::join_paths(filtered)
            .unwrap()
            .into_string()
            .unwrap();
        unsafe { std::env::set_var("PATH", filtered_path) };
        Self {
            _lock: lock,
            orig_path,
        }
    }
}

impl Drop for PathGuard {
    fn drop(&mut self) {
        unsafe { std::env::set_var("PATH", &self.orig_path) };
    }
}

#[test]
fn structural_tools_absent_when_no_backend() {
    let dir = scratch("struct-absent");
    let _guard = PathGuard::without_backends();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);
    let names: Vec<String> = schemas(&tools)
        .into_iter()
        .map(|s| s.name().to_string())
        .collect();

    assert!(!names.contains(&"sym_skeleton".to_string()));
    assert!(!names.contains(&"sym_callers".to_string()));
    assert!(!names.contains(&"sym_impact".to_string()));
    assert!(!names.contains(&"sym_affected_tests".to_string()));
    assert!(!names.contains(&"atlas_edit".to_string()));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn structural_tools_polydex_fresh_registration_and_dispatch() {
    let dir = scratch("polydex-fresh");
    let body = r#"
case "$1" in
    status) echo '{"exists":true,"freshness":{"new_files":[],"modified_files":[],"deleted_files":[]}}' ;;
    skeleton) echo '{"symbols":[{"name":"test_sym","kind":"function","line":10}]}' ;;
    callers) echo '{"callers":["caller_fn"]}' ;;
    impact) echo '{"impact":["downstream_fn"]}' ;;
    affected-tests) echo '{"tests":["test_alpha"]}' ;;
    *) echo "unknown cmd $1" >&2; exit 1 ;;
esac
"#;
    fake_bin(&dir, "polydex", body);
    let _guard = PathGuard::with_dir(&dir);

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    // Verify registration
    let names: Vec<String> = schemas(&tools)
        .into_iter()
        .map(|s| s.name().to_string())
        .collect();
    assert!(names.contains(&"sym_skeleton".to_string()));
    assert!(names.contains(&"sym_callers".to_string()));
    assert!(names.contains(&"sym_impact".to_string()));
    assert!(names.contains(&"sym_affected_tests".to_string()));

    // Dispatch sym_skeleton
    let res = dispatch(&tools, &ctx, "sym_skeleton", &json!({"file": "src/lib.rs"})).unwrap();
    assert!(res.contains("test_sym"));

    // Dispatch sym_callers
    let res = dispatch(&tools, &ctx, "sym_callers", &json!({"name": "test_sym"})).unwrap();
    assert!(res.contains("caller_fn"));

    // Dispatch sym_impact
    let res = dispatch(&tools, &ctx, "sym_impact", &json!({"symbol": "test_sym"})).unwrap();
    assert!(res.contains("downstream_fn"));

    // Dispatch sym_affected_tests
    let res = dispatch(
        &tools,
        &ctx,
        "sym_affected_tests",
        &json!({"changed": ["test_sym"]}),
    )
    .unwrap();
    assert!(res.contains("test_alpha"));

    // Missing arguments error checks
    assert!(dispatch(&tools, &ctx, "sym_skeleton", &json!({}))
        .unwrap_err()
        .to_string()
        .contains("file required"));
    assert!(dispatch(&tools, &ctx, "sym_callers", &json!({}))
        .unwrap_err()
        .to_string()
        .contains("name required"));
    assert!(dispatch(&tools, &ctx, "sym_impact", &json!({}))
        .unwrap_err()
        .to_string()
        .contains("symbol required"));
    assert!(dispatch(&tools, &ctx, "sym_affected_tests", &json!({}))
        .unwrap_err()
        .to_string()
        .contains("changed required"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn structural_tools_polydex_stale_returns_fallback_note() {
    let dir = scratch("polydex-stale");
    let body = r#"
case "$1" in
    status) echo '{"exists":true,"freshness":{"new_files":["src/mod.rs"],"modified_files":[],"deleted_files":[]}}' ;;
    skeleton) echo "should-not-run" ;;
    *) exit 0 ;;
esac
"#;
    fake_bin(&dir, "polydex", body);
    let _guard = PathGuard::with_dir(&dir);

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(&tools, &ctx, "sym_skeleton", &json!({"file": "src/lib.rs"})).unwrap();
    assert!(
        res.contains("STALE INDEX"),
        "stale index must return advisory note: {res}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn structural_tools_code_atlas_registration_and_dispatch() {
    let dir = scratch("atlas-test");
    let body = r#"
if [ "$1" = "edit" ]; then
    echo "--- a/file.rs\n+++ b/file.rs\n@@ -1 +1 @@\n-old\n+new"
    exit 0
fi
echo "unknown" >&2
exit 2
"#;
    fake_bin(&dir, "code-atlas", body);
    let _guard = PathGuard::with_dir(&dir);

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    // atlas_edit registered
    let names: Vec<String> = schemas(&tools)
        .into_iter()
        .map(|s| s.name().to_string())
        .collect();
    assert!(names.contains(&"atlas_edit".to_string()));

    // Successful dispatch
    let res = dispatch(
        &tools,
        &ctx,
        "atlas_edit",
        &json!({"path": "src/lib.rs", "old": "old", "new": "new"}),
    )
    .unwrap();
    assert!(res.contains("--- a/file.rs"));
    assert!(res.contains("+new"));

    // Missing arguments
    assert!(
        dispatch(&tools, &ctx, "atlas_edit", &json!({"old": "o", "new": "n"}))
            .unwrap_err()
            .to_string()
            .contains("path required")
    );
    assert!(dispatch(
        &tools,
        &ctx,
        "atlas_edit",
        &json!({"path": "p", "new": "n"})
    )
    .unwrap_err()
    .to_string()
    .contains("old required"));
    assert!(dispatch(
        &tools,
        &ctx,
        "atlas_edit",
        &json!({"path": "p", "old": "o"})
    )
    .unwrap_err()
    .to_string()
    .contains("new required"));

    // Atlas edit failure surfaces exit code and stderr
    let fail_body = r#"echo "ambiguous region" >&2; exit 3"#;
    fake_bin(&dir, "code-atlas", fail_body);
    let err = dispatch(
        &tools,
        &ctx,
        "atlas_edit",
        &json!({"path": "src/lib.rs", "old": "old", "new": "new"}),
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("atlas_edit exited with exit status: 3"));
    assert!(msg.contains("ambiguous region"));

    std::fs::remove_dir_all(&dir).ok();
}
