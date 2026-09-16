//! The search tools: glob (suffix/pattern file listing) + grep
//! (case-insensitive content search) over a bounded worktree walk.

use super::{resolve, Tool, ToolCtx};
use crate::llm::{truncate, ToolSchema};
use anyhow::Context;
use serde_json::json;
use std::path::Path;

pub(super) fn glob_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "glob",
            "List files matching a pattern (e.g. \"src/**/*.rs\"), .gitignore-aware-ish, capped at 200 hits.",
            json!({"type":"object","properties":{"pattern":{"type":"string","description":"suffix match, e.g. .rs or src/"}},"required":["pattern"]}),
        ),
        handler: Box::new(move |c, a| {
            let pat = a["pattern"].as_str().context("pattern required")?;
            let mut hits = vec![];
            walk(c.wt, 0, &mut |p| {
                let rel = p.strip_prefix(c.wt).unwrap_or(p).display().to_string();
                if hits.len() < 200 && (rel.contains(pat) || rel.ends_with(pat) || pat == "*") {
                    hits.push(rel);
                }
                hits.len() < 200
            });
            hits.sort();
            Ok(if hits.is_empty() { "no matches".into() } else { hits.join("\n") })
        }),
    }
}
fn walk(dir: &Path, depth: usize, f: &mut dyn FnMut(&Path) -> bool) -> bool {
    if depth > 12 {
        return true;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return true;
    };
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let is_dir = p.is_dir();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if name == ".git" || name == "node_modules" || (is_dir && name.starts_with("target")) {
            continue;
        }
        if is_dir {
            if !walk(&p, depth + 1, f) {
                return false;
            }
        } else if !f(&p) {
            return false;
        }
    }
    true
}

pub(super) fn grep_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "grep",
            "Search file contents for a pattern (plain text, case-insensitive). Capped.",
            json!({"type":"object","properties":{"pattern":{"type":"string"},"path":{"type":"string","description":"subdir to search (default whole worktree)"}},"required":["pattern"]}),
        ),
        handler: Box::new(move |c, a| {
            let pat = a["pattern"]
                .as_str()
                .context("pattern required")?
                .to_lowercase();
            let root = match a["path"].as_str() {
                Some(p) => resolve(c, p)?,
                None => c.wt.to_path_buf(),
            };
            let mut out = vec![];
            walk(&root, 0, &mut |p| {
                if out.len() >= 80 {
                    return false;
                }
                if let Ok(txt) = std::fs::read_to_string(p) {
                    if p.extension()
                        .map(|e| {
                            e == "rs"
                                || e == "md"
                                || e == "toml"
                                || e == "json"
                                || e == "py"
                                || e == "ts"
                                || e == "js"
                                || e == "go"
                                || e == "c"
                                || e == "h"
                        })
                        .unwrap_or(false)
                    {
                        for (i, line) in txt.lines().enumerate() {
                            if line.to_lowercase().contains(&pat) {
                                out.push(format!(
                                    "{}:{}: {}",
                                    p.strip_prefix(c.wt).unwrap_or(p).display(),
                                    i + 1,
                                    truncate(line.trim(), 160)
                                ));
                                break; // one hit per file keeps the tool result small
                            }
                        }
                    }
                }
                out.len() < 80
            });
            Ok(if out.is_empty() {
                "no matches".into()
            } else {
                out.join("\n")
            })
        }),
    }
}
