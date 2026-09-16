//! The bash tool and its safe-exec plumbing: the destructive guard
//! (jokersquad semantics — hard-deny, modes warn/deny/off), the sandbox
//! tier ladder (strict/moderate = bwrap, free = plain subprocess), and the
//! `bash` tool that routes through both.

use super::{Tool, ToolCtx};
use crate::llm::{truncate, ToolSchema};
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::Path;
use std::process::Command;

/// jokersquad safe-exec semantics: hard-deny destructive patterns, modes
/// warn/deny/off (default deny). Exit 99 on hard-deny is upstream's shape.
pub(super) fn destructive_guard(cmd: &str) -> Option<&'static str> {
    const DENY: &[(&str, &str)] = &[
        ("rm -rf /", "rm -rf /"),
        ("rm -rf ~", "rm -rf ~"),
        ("rm -rf .", "rm -rf ."),
        ("git push --force", "git push --force"),
        ("git push -f", "git push -f"),
        ("git reset --hard", "git reset --hard"),
        ("git checkout .", "git checkout ."),
        ("sudo ", "sudo"),
        ("dd if=", "dd"),
        ("mkfs", "mkfs"),
        ("shutdown", "shutdown"),
        ("reboot", "reboot"),
        (":(){:|:&", "fork bomb"),
        ("chmod -R 777 /", "chmod -R 777 /"),
    ];
    let lower = cmd.to_lowercase();
    DENY.iter()
        .find(|(p, _)| lower.contains(p))
        .map(|(_, why)| *why)
}

/// The bash tool's execution path (PHEOBE-14): destructive guard in EVERY
/// tier, then the sandbox ladder (strict = bwrap w/o network + allowlist,
/// moderate = bwrap w/ network, free = plain subprocess).
pub(super) fn run_bash_tiered(
    cmd: &str,
    cwd: &Path,
    tier: &crate::sandbox::Tier,
    timeout_note: &str,
) -> Result<String> {
    let _ = timeout_note;
    match std::env::var("PHEOBE_SAFE_EXEC_MODE").ok().as_deref() {
        Some("off") => {}
        Some("warn") => {
            if let Some(why) = destructive_guard(cmd) {
                eprintln!("safe-exec warn: would deny '{why}' — running anyway");
            }
        }
        _ => {
            if let Some(why) = destructive_guard(cmd) {
                bail!("safe-exec deny: {why} — blocked before exec (PHEOBE_SAFE_EXEC_MODE=warn to allow)")
            }
        }
    }
    // strict tier: only test/build/done_when-style commands pass the allowlist
    if matches!(tier, crate::sandbox::Tier::Strict) && !crate::sandbox::strict_allowed(cmd) {
        bail!(
            "sandbox strict tier: command not on the allowlist ({cmd:?}) — only \
             test/build/done_when prefixes (cargo test, cargo build, npm test, pytest, \
             python3, go test, node, sh -c) are permitted; set PHEOBE_SANDBOX=moderate \
             for general shell"
        );
    }
    // bwrap isolation for strict/moderate; free (or missing bwrap on moderate)
    // runs a plain guarded subprocess
    match crate::sandbox::sandboxed_command(cmd, cwd, tier)? {
        Some(mut child) => {
            let out = child.output()?;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let code = out.status.code().unwrap_or(-1);
            Ok(format!(
                "[sandbox exit {code}]\n{}",
                truncate(combined.trim_end(), 8000)
            ))
        }
        None => {
            let out = Command::new("sh")
                .args(["-c", cmd])
                .current_dir(cwd)
                .output()?;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let code = out.status.code().unwrap_or(-1);
            Ok(format!(
                "[exit {code}]\n{}",
                truncate(combined.trim_end(), 8000)
            ))
        }
    }
}
pub(super) fn bash_tool(_ctx: &ToolCtx<'_>) -> Tool {
    Tool {
        schema: ToolSchema::new(
            "bash",
            "Run a shell command in the worktree. Output truncated to 8KB. Destructive commands are blocked (safe-exec). Commands run sandboxed per the run's tier (strict = no network + allowlist).",
            json!({"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}),
        ),
        handler: Box::new(move |c, a| {
            let cmd = a["command"].as_str().context("command required")?;
            let tier = crate::sandbox::Tier::from_name(&c.task.effective_sandbox()?)?;
            run_bash_tiered(cmd, c.wt, &tier, "bash")
        }),
    }
}
