//! The optional structural-ladder tools (PHEOBE-11): registered only when
//! their backend binary is on PATH at barn build time — absent = skipped,
//! never failed.

use super::{Tool, ToolCtx};
use crate::llm::ToolSchema;
use anyhow::{bail, Context};
use serde_json::{json, Value};

/// The optional structural-ladder tools (PHEOBE-11): registered only when
/// their backend binary is on PATH at barn build time — absent = skipped,
/// never failed. The read ladder upgrades text coordinates to structural
/// ones (skeleton/enclosing/callers/impact/affected-tests). The code-atlas
/// write ladder is gate-ready: `atlas_edit` registers only when code-atlas
/// exists; on this box it is design-only, so the ladder stays empty and the
/// honest fallback (exact-string `edit`) is what models call.
pub(super) fn structural_tools(_ctx: &ToolCtx<'_>) -> Vec<Tool> {
    let mut t = Vec::new();
    if crate::structint::polydex_bin().is_some() {
        t.push(sym_tool(
            "sym_skeleton",
            "Signatures-only view of one file from the polydex index (symbols, kinds, parents, lines) — the cheap structural read before opening the whole file. Stale index → fall back to `read` and record the doubt.",
            json!({"type":"object","properties":{"file":{"type":"string"}},"required":["file"]}),
            |c, a| crate::structint::skeleton(c.wt, a["file"].as_str().context("file required")?),
        ));
        t.push(sym_tool(
            "sym_callers",
            "Direct callers of a symbol from the polydex call graph (incoming calls edges). Stale index → fall back to `grep` and record the doubt.",
            json!({"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}),
            |c, a| crate::structint::callers(c.wt, a["name"].as_str().context("name required")?),
        ));
        t.push(sym_tool(
            "sym_impact",
            "Transitive callers of a symbol — what breaks if I change this. Stale index → fall back to `grep` and record the doubt.",
            json!({"type":"object","properties":{"symbol":{"type":"string"}},"required":["symbol"]}),
            |c, a| crate::structint::impact(c.wt, a["symbol"].as_str().context("symbol required")?),
        ));
        t.push(sym_tool(
            "sym_affected_tests",
            "Test symbols transitively reachable from the changed symbols — the preferred pre-check before running the suite. Stale index → fall back to `verify` with the whole suite and record the doubt.",
            json!({"type":"object","properties":{"changed":{"type":"array","items":{"type":"string"}}},"required":["changed"]}),
            |c, a| {
                let changed: Vec<String> = a["changed"]
                    .as_array()
                    .context("changed required")?
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();
                crate::structint::affected_tests(c.wt, &changed)
            },
        ));
    }
    if crate::structint::code_atlas_bin().is_some() {
        t.push(sym_tool(
            "atlas_edit",
            "code-atlas structural edit: stable region handle + two-hash guard + parse-gated atomic write, ALWAYS returns the diff. Ambiguity is an error: report blocked rather than guess. Falls back to `edit` semantics if the region is not uniquely resolvable.",
            json!({"type":"object","properties":{"path":{"type":"string"},"old":{"type":"string"},"new":{"type":"string"}},"required":["path","old","new"]}),
            |c, a| {
                let bin = crate::structint::code_atlas_bin().context("code-atlas unavailable")?;
                let out = std::process::Command::new(&bin)
                    .args(["edit", "--file", a["path"].as_str().context("path required")?])
                    .args(["--old", a["old"].as_str().context("old required")?])
                    .args(["--new", a["new"].as_str().context("new required")?])
                    .current_dir(c.wt)
                    .output()
                    .context("code-atlas edit failed to spawn")?;
                if !out.status.success() {
                    bail!(
                        "atlas_edit exited with {}: {}",
                        out.status,
                        String::from_utf8_lossy(&out.stderr).trim()
                    );
                }
                Ok(Some(String::from_utf8_lossy(&out.stdout).into_owned()))
            },
        ));
    }
    t
}

/// Build one structural tool: the freshness gate lives in
/// `structint::read_fresh`, which returns `Ok(None)` only when the backend
/// is absent (the tool should not have been registered) and surfaces the
/// stale-index note as its own result so the model records the doubt.
fn sym_tool(
    name: &str,
    description: &str,
    parameters: Value,
    run: impl for<'a> Fn(&ToolCtx<'a>, &Value) -> anyhow::Result<Option<String>> + Send + Sync + 'static,
) -> Tool {
    let owned_name = name.to_string();
    Tool {
        schema: ToolSchema::new(name, description, parameters),
        handler: Box::new(move |c, a| match run(c, a) {
            Ok(Some(text)) => Ok(text),
            Ok(None) => bail!("{owned_name}: backend unavailable — use the text-tool fallback"),
            Err(e) => Err(e),
        }),
    }
}
