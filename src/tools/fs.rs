//! The file tools: read / write / edit — all behind the shared path gate
//! (worktree jail + paths_allow) and the write-time formatter.

use super::{check_allow, resolve, Tool, ToolCtx};
use crate::llm::ToolSchema;
use anyhow::{bail, Context};
use serde_json::json;

pub(super) fn read_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "read",
            "Read a file (text, size/line-limited). Path is worktree-relative or absolute inside the worktree.",
            json!({"type":"object","properties":{"path":{"type":"string"},"offset":{"type":"integer","description":"1-based start line"},"limit":{"type":"integer","description":"max lines (default 400)"}},"required":["path"]}),
        ),
        handler: Box::new(move |c, a| {
            let p = resolve(c, a["path"].as_str().context("path required")?)?;
            let txt = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
            let offset = a["offset"].as_u64().unwrap_or(1).max(1) as usize;
            let limit = a["limit"].as_u64().unwrap_or(400) as usize;
            let lines: Vec<&str> = txt.lines().skip(offset - 1).take(limit).collect();
            Ok(format!("{}{}:{}", p.display(), if lines.len() == limit { "+ (truncated)" } else { "" }, lines.join("\n")))
        }),
    }
}

pub(super) fn write_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "write",
            "Write a file (overwrites). Only inside the worktree and paths_allow.",
            json!({"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}),
        ),
        handler: Box::new(move |c, a| {
            let p = resolve(c, a["path"].as_str().context("path required")?)?;
            check_allow(c, &p)?;
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&p, a["content"].as_str().context("content required")?)?;
            let note = crate::fmt::format_on_write(c.wt, &p).unwrap_or_default();
            Ok(format!(
                "wrote {} ({} bytes){note}",
                p.display(),
                a["content"].as_str().unwrap_or("").len()
            ))
        }),
    }
}

pub(super) fn edit_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "edit",
            "Exact-string replace in a file. old_string must be unique; ambiguity is an error, never resolved silently.",
            json!({"type":"object","properties":{"path":{"type":"string"},"old_string":{"type":"string"},"new_string":{"type":"string"}},"required":["path","old_string","new_string"]}),
        ),
        handler: Box::new(move |c, a| {
            let p = resolve(c, a["path"].as_str().context("path required")?)?;
            check_allow(c, &p)?;
            let old = a["old_string"].as_str().context("old_string required")?;
            let new = a["new_string"].as_str().context("new_string required")?;
            let txt = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
            let n = txt.matches(old).count();
            if n == 0 {
                bail!("old_string not found in {} — read the file again", p.display());
            }
            if n > 1 {
                bail!("old_string matches {n} times in {} — ambiguous edit refused; add more context", p.display());
            }
            let updated = txt.replacen(old, new, 1);
            std::fs::write(&p, &updated)?;
            let note = crate::fmt::format_on_write(c.wt, &p).unwrap_or_default();
            Ok(format!("edited {} (+{}-{} chars){note}", p.display(), new.len(), old.len()))
        }),
    }
}
