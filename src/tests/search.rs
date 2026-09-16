//! PHEOBE-35: Barn search tools tests (`tools/search.rs`).
//! Exercises glob and grep tool schemas and execution behaviors.

use crate::task::Task;
use crate::tools::{barn, dispatch, ToolCtx};
use serde_json::json;
use std::path::{Path, PathBuf};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "pheobe-search-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn test_task() -> Task {
    serde_json::from_str(r#"{"task":"test","done_when":{"type":"command","run":"true"}}"#).unwrap()
}

fn test_ctx<'a>(wt: &'a Path, task: &'a Task) -> ToolCtx<'a> {
    ToolCtx {
        wt,
        task,
        task_id: "test-task-search",
    }
}

// ── glob_tool tests ─────────────────────────────────────────────────────────

#[test]
fn glob_schema_conforms_to_contract() {
    let dir = scratch("glob-schema");
    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);
    let glob = tools
        .iter()
        .find(|t| t.schema.name() == "glob")
        .expect("glob tool must be in barn");

    assert_eq!(glob.schema.name(), "glob");
    let val = serde_json::to_value(&glob.schema).unwrap();
    let func = &val["function"];
    assert_eq!(func["name"], "glob");
    let desc = func["description"].as_str().unwrap();
    assert!(desc.contains("List files matching a pattern"));
    assert!(desc.contains("capped at 200 hits"));

    let params = &func["parameters"];
    assert_eq!(params["type"], "object");
    assert_eq!(params["required"], json!(["pattern"]));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn glob_pattern_matching_and_sorting() {
    let dir = scratch("glob-match");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "// lib").unwrap();
    std::fs::write(dir.join("src/main.rs"), "// main").unwrap();
    std::fs::write(dir.join("docs/readme.md"), "# Readme").unwrap();
    std::fs::write(dir.join("build.rs"), "// build").unwrap();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    // suffix match
    let res = dispatch(&tools, &ctx, "glob", &json!({"pattern": ".rs"})).unwrap();
    assert_eq!(res, "build.rs\nsrc/lib.rs\nsrc/main.rs");

    // substring match
    let res = dispatch(&tools, &ctx, "glob", &json!({"pattern": "src/"})).unwrap();
    assert_eq!(res, "src/lib.rs\nsrc/main.rs");

    // wildcard match
    let res = dispatch(&tools, &ctx, "glob", &json!({"pattern": "*"})).unwrap();
    assert_eq!(res, "build.rs\ndocs/readme.md\nsrc/lib.rs\nsrc/main.rs");

    // no match
    let res = dispatch(&tools, &ctx, "glob", &json!({"pattern": ".py"})).unwrap();
    assert_eq!(res, "no matches");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn glob_missing_pattern_errors() {
    let dir = scratch("glob-err");
    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let err = dispatch(&tools, &ctx, "glob", &json!({})).unwrap_err();
    assert!(err.to_string().contains("pattern required"));

    let err = dispatch(&tools, &ctx, "glob", &json!({"pattern": 42})).unwrap_err();
    assert!(err.to_string().contains("pattern required"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn glob_skips_git_target_node_modules() {
    let dir = scratch("glob-skips");
    std::fs::create_dir_all(dir.join(".git/objects")).unwrap();
    std::fs::create_dir_all(dir.join("target/debug")).unwrap();
    std::fs::create_dir_all(dir.join("node_modules/pkg")).unwrap();
    std::fs::create_dir_all(dir.join("src")).unwrap();

    std::fs::write(dir.join(".git/config"), "git").unwrap();
    std::fs::write(dir.join("target/debug/app"), "bin").unwrap();
    std::fs::write(dir.join("node_modules/pkg/index.js"), "js").unwrap();
    std::fs::write(dir.join("src/app.rs"), "app").unwrap();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(&tools, &ctx, "glob", &json!({"pattern": "*"})).unwrap();
    assert_eq!(res, "src/app.rs");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn glob_depth_limit_greater_than_12() {
    let dir = scratch("glob-depth");
    let mut curr = dir.clone();
    for i in 1..=14 {
        curr = curr.join(format!("d{i}"));
        std::fs::create_dir_all(&curr).unwrap();
        std::fs::write(curr.join("file.rs"), "ok").unwrap();
    }

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(&tools, &ctx, "glob", &json!({"pattern": ".rs"})).unwrap();
    let lines: Vec<&str> = res.lines().collect();
    // Depth > 12 is skipped, so d1..d13 exists, d14 should not appear
    assert!(
        lines.iter().any(|l| l.contains("d12")),
        "depth 12 should be visited"
    );
    assert!(
        !lines.iter().any(|l| l.contains("d14")),
        "depth > 12 should be skipped"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn glob_caps_at_200_hits() {
    let dir = scratch("glob-cap");
    std::fs::create_dir_all(dir.join("files")).unwrap();
    for i in 0..215 {
        std::fs::write(dir.join(format!("files/{i:03}.rs")), "data").unwrap();
    }

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(&tools, &ctx, "glob", &json!({"pattern": ".rs"})).unwrap();
    let lines: Vec<&str> = res.lines().collect();
    assert_eq!(lines.len(), 200, "glob hits must be capped at 200");

    std::fs::remove_dir_all(&dir).ok();
}

// ── grep_tool tests ─────────────────────────────────────────────────────────

#[test]
fn grep_schema_conforms_to_contract() {
    let dir = scratch("grep-schema");
    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);
    let grep = tools
        .iter()
        .find(|t| t.schema.name() == "grep")
        .expect("grep tool must be in barn");

    assert_eq!(grep.schema.name(), "grep");
    let val = serde_json::to_value(&grep.schema).unwrap();
    let func = &val["function"];
    assert_eq!(func["name"], "grep");
    let desc = func["description"].as_str().unwrap();
    assert!(desc.contains("Search file contents for a pattern"));
    assert!(desc.contains("plain text, case-insensitive"));

    let params = &func["parameters"];
    assert_eq!(params["type"], "object");
    assert_eq!(params["required"], json!(["pattern"]));
    assert!(params["properties"].get("path").is_some());
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_plain_text_case_insensitive_and_no_regex() {
    let dir = scratch("grep-case");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(
        dir.join("src/app.rs"),
        "fn HelloWorld() -> i32 {\n    42\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("src/literal.rs"),
        "// regex special characters: fn.*world\n",
    )
    .unwrap();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    // Case insensitive match
    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": "helloworld"})).unwrap();
    assert!(res.contains("src/app.rs:1: fn HelloWorld() -> i32 {"));

    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": "HELLOWORLD"})).unwrap();
    assert!(res.contains("src/app.rs:1: fn HelloWorld() -> i32 {"));

    // Plain text matching: `fn.*world` does NOT match `fn HelloWorld()`, only literal
    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": "fn.*world"})).unwrap();
    assert_eq!(
        res,
        "src/literal.rs:1: // regex special characters: fn.*world"
    );

    // No matches returns clean message
    let res = dispatch(
        &tools,
        &ctx,
        "grep",
        &json!({"pattern": "nonexistent_term"}),
    )
    .unwrap();
    assert_eq!(res, "no matches");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_file_extension_filtering() {
    let dir = scratch("grep-exts");
    let hit = "FIND_ME_TARGET";

    // Supported extensions: rs, md, toml, json, py, ts, js, go, c, h
    let supported = [
        "a.rs", "b.md", "c.toml", "d.json", "e.py", "f.ts", "g.js", "h.go", "i.c", "j.h",
    ];
    for name in &supported {
        std::fs::write(dir.join(name), format!("{hit} inside {name}\n")).unwrap();
    }

    // Unsupported extensions / no ext: txt, bin, log, noext
    let unsupported = ["k.txt", "l.bin", "m.log", "LICENSE"];
    for name in &unsupported {
        std::fs::write(dir.join(name), format!("{hit} inside {name}\n")).unwrap();
    }

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": hit})).unwrap();
    for name in &supported {
        assert!(
            res.contains(name),
            "expected {name} to be searched, got:\n{res}"
        );
    }
    for name in &unsupported {
        assert!(
            !res.contains(name),
            "expected {name} to be ignored, got:\n{res}"
        );
    }

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_path_scoping_and_jail() {
    let dir = scratch("grep-path");
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::create_dir_all(dir.join("docs")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "COMMON_KEYWORD").unwrap();
    std::fs::write(dir.join("docs/guide.md"), "COMMON_KEYWORD").unwrap();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    // Scoped to src
    let res = dispatch(
        &tools,
        &ctx,
        "grep",
        &json!({"pattern": "COMMON_KEYWORD", "path": "src"}),
    )
    .unwrap();
    assert!(res.contains("src/lib.rs"));
    assert!(!res.contains("docs/guide.md"));

    // Scoped to docs
    let res = dispatch(
        &tools,
        &ctx,
        "grep",
        &json!({"pattern": "COMMON_KEYWORD", "path": "docs"}),
    )
    .unwrap();
    assert!(!res.contains("src/lib.rs"));
    assert!(res.contains("docs/guide.md"));

    // Outside path error
    let err = dispatch(
        &tools,
        &ctx,
        "grep",
        &json!({"pattern": "COMMON_KEYWORD", "path": "/etc"}),
    )
    .unwrap_err();
    assert!(err.to_string().contains("escapes the worktree root"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_one_hit_per_file_and_truncation() {
    let dir = scratch("grep-truncate");
    let long_suffix = "x".repeat(200);
    let content = format!(
        "first hit on line 1: TARGET\nsecond hit on line 2: TARGET\nthird hit: TARGET {long_suffix}\n"
    );
    std::fs::write(dir.join("file.rs"), content).unwrap();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": "TARGET"})).unwrap();
    let lines: Vec<&str> = res.lines().collect();
    assert_eq!(
        lines.len(),
        1,
        "grep must only return one hit per file (stops on first)"
    );
    assert!(lines[0].starts_with("file.rs:1:"));

    // Test line truncation with line 1 being long
    let long_line = format!("TARGET {}", "y".repeat(250));
    std::fs::write(dir.join("long.rs"), format!("{long_line}\n")).unwrap();
    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": "TARGET"})).unwrap();
    let long_hit = res
        .lines()
        .find(|l| l.starts_with("long.rs:"))
        .expect("long.rs hit");
    assert!(
        long_hit.contains("…[") && long_hit.ends_with("bytes truncated]"),
        "line >160 chars must have truncation note, got: {long_hit}"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn search_walk_includes_files_starting_with_target_issue_11() {
    let dir = scratch("issue-11-target");
    std::fs::create_dir_all(dir.join("target-build")).unwrap();
    std::fs::write(dir.join("target-build/sub.rs"), "MATCH_ME in skipped dir\n").unwrap();
    std::fs::write(dir.join("target.rs"), "MATCH_ME in target file\n").unwrap();
    std::fs::write(dir.join("targeting.py"), "MATCH_ME in targeting file\n").unwrap();
    std::fs::write(dir.join("valid.rs"), "MATCH_ME in valid file\n").unwrap();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    // Issue 11 fix: target.rs and targeting.py are regular files, so they are searched;
    // target-build is a directory starting with "target", so it is skipped.
    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": "MATCH_ME"})).unwrap();
    assert_eq!(
        res,
        "target.rs:1: MATCH_ME in target file\ntargeting.py:1: MATCH_ME in targeting file\nvalid.rs:1: MATCH_ME in valid file"
    );

    let glob_res = dispatch(&tools, &ctx, "glob", &json!({"pattern": "*"})).unwrap();
    assert_eq!(glob_res, "target.rs\ntargeting.py\nvalid.rs");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_caps_at_80_hits() {
    let dir = scratch("grep-cap");
    std::fs::create_dir_all(dir.join("files")).unwrap();
    for i in 0..90 {
        std::fs::write(
            dir.join(format!("files/f{i:03}.rs")),
            "NEEDLE_IN_HAYSTACK\n",
        )
        .unwrap();
    }

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(
        &tools,
        &ctx,
        "grep",
        &json!({"pattern": "NEEDLE_IN_HAYSTACK"}),
    )
    .unwrap();
    let lines: Vec<&str> = res.lines().collect();
    assert_eq!(lines.len(), 80, "grep hits must be capped at 80 files");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn grep_skips_binary_and_missing_pattern() {
    let dir = scratch("grep-binary");
    // Binary file with invalid UTF-8 with .rs extension
    std::fs::write(dir.join("bad.rs"), [0xFF, 0xFE, 0x00, 0xAA]).unwrap();
    std::fs::write(dir.join("good.rs"), "valid text SEARCH_ME\n").unwrap();

    let task = test_task();
    let ctx = test_ctx(&dir, &task);
    let tools = barn(&ctx);

    let res = dispatch(&tools, &ctx, "grep", &json!({"pattern": "SEARCH_ME"})).unwrap();
    assert_eq!(res, "good.rs:1: valid text SEARCH_ME");

    let err = dispatch(&tools, &ctx, "grep", &json!({})).unwrap_err();
    assert!(err.to_string().contains("pattern required"));

    std::fs::remove_dir_all(&dir).ok();
}
