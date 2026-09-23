//! Task schema (v0) + intake validation. `done_when` reuses mayfly's kinds.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    /// The ask. One purpose per run.
    pub task: String,
    /// The mechanical success gate.
    pub done_when: DoneWhen,
    /// Repo to cook in (never edited in place — see worktree).
    #[serde(default)]
    pub repo: Option<PathBuf>,
    #[serde(default = "default_true")]
    pub worktree: bool,
    #[serde(default)]
    pub branch: Option<String>,
    /// Self-set deadline, honored. e.g. "45m", "2h".
    #[serde(default)]
    pub ttl: Option<String>,
    #[serde(default)]
    pub budget: Option<Budget>,
    /// Paths this run may touch; enforced at tool level and pre-commit.
    #[serde(default)]
    pub paths_allow: Vec<String>,
    #[serde(default)]
    pub push: bool,
    /// Sandbox isolation for bash commands. Values: `strict`, `moderate`,
    /// `free`. Default when unset and no env override = `moderate`.
    /// `PHEOBE_SANDBOX` env var always wins when set (even over an
    /// explicit task value — env is the operator override).
    #[serde(default)]
    pub sandbox: Option<String>,
    /// Engine model for worker adapters (PHEOBE-41 claude; PHEOBE-46 every
    /// engine: opencode/codex/kimi `-m`, cursor/agy `--model`). The adapter's
    /// own env override (`PHEOBE_<ENGINE>_MODEL`) wins over this, as
    /// `PHEOBE_SANDBOX` wins over `sandbox`. Ignored by the built-in loop
    /// (that uses `PHEOBE_MODEL`).
    #[serde(default)]
    pub model: Option<String>,
    /// Engine for this task (PHEOBE-46): `openai` (the built-in loop) or a
    /// registered worker (`opencode`, `claude`, `claude-sdk`, `codex`,
    /// `cursor`, `kimi`, `agy`). `PHEOBE_PROVIDER` wins over it — the operator
    /// override, the same rule as `sandbox`/`model`.
    #[serde(default)]
    pub provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DoneWhen {
    /// Shell command exits with `expect_exit` (default 0).
    Command {
        run: String,
        #[serde(default = "default_expect_exit")]
        expect_exit: i32,
    },
    /// Every listed path (relative to the worktree) exists (PHEOBE-44).
    FilesExist { paths: Vec<String> },
}

fn default_true() -> bool {
    true
}
fn default_expect_exit() -> i32 {
    0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Budget {
    #[serde(default = "default_max_iterations")]
    pub max_iterations: u32,
    #[serde(default)]
    pub max_usd: Option<f64>,
}

fn default_max_iterations() -> u32 {
    8
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            max_iterations: default_max_iterations(),
            max_usd: None,
        }
    }
}

/// Parse the task file (path or `-` for stdin).
pub fn load(path: &str) -> Result<Task> {
    let raw = if path == "-" {
        use std::io::Read;
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s)?;
        s
    } else {
        std::fs::read_to_string(path)?
    };
    let task: Task = serde_json::from_str(&raw)?;
    task.validate()?;
    Ok(task)
}

/// Parse task JSON from a string (the ACP dispatch path — the prompt text
/// carries a pheobe task JSON, no file involved).
pub fn load_from_str(raw: &str) -> Result<Task> {
    let task: Task = serde_json::from_str(raw)?;
    task.validate()?;
    Ok(task)
}

impl Task {
    /// Resolve the effective sandbox tier: `PHEOBE_SANDBOX` env (always wins)
    /// → `task.sandbox` → `"moderate"` (default). Unknown tier = intake error.
    pub fn effective_sandbox(&self) -> Result<String> {
        let raw = std::env::var("PHEOBE_SANDBOX")
            .ok()
            .or_else(|| self.sandbox.clone())
            .unwrap_or_else(|| "moderate".into());
        let tier = raw.trim().to_ascii_lowercase();
        match tier.as_str() {
            "strict" | "moderate" | "free" => Ok(tier),
            other => bail!(
                "unknown sandbox tier '{other}' — PHEOBE_SANDBOX must be strict | moderate | free"
            ),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.task.trim().is_empty() {
            bail!("task is empty");
        }
        match &self.done_when {
            DoneWhen::Command { run, .. } if run.trim().is_empty() => {
                bail!(
                    "done_when.run is empty — a run without a mechanical gate is not a pheobe task"
                )
            }
            DoneWhen::FilesExist { paths } if paths.iter().all(|p| p.trim().is_empty()) => {
                bail!("done_when.paths is empty — files_exist needs at least one path")
            }
            _ => {}
        }
        // Fuzziness gate (mayfly's rule, intentionally strict). Heuristics, not law —
        // but a vague ask dies here rather than burning budget.
        let t = self.task.to_lowercase();
        for verb in [
            "polish",
            "improve",
            "rethink",
            "make better",
            "clean up the whole",
        ] {
            if t.contains(verb) {
                bail!("vague ask: contains '{verb}' — state the concrete change and done_when");
            }
        }
        if t.contains(" and also ") {
            bail!("multiple top-level goals — one purpose per run");
        }
        if let Some(p) = &self.provider {
            if !crate::worker::is_known_provider(p) {
                bail!(
                    "unknown provider '{p}' — one of: openai, {}",
                    crate::worker::provider_names().join(", ")
                );
            }
        }
        Ok(())
    }

    /// The engine this task runs on: `PHEOBE_PROVIDER` > task `provider` >
    /// `openai` (PHEOBE-46).
    pub fn effective_provider(&self) -> String {
        std::env::var("PHEOBE_PROVIDER")
            .ok()
            .filter(|p| !p.trim().is_empty())
            .or_else(|| self.provider.clone())
            .unwrap_or_else(|| "openai".to_string())
    }

    pub fn resolve_repo(&self) -> Result<PathBuf> {
        match &self.repo {
            Some(p) => Ok(p.clone()),
            None => {
                if let Ok(cwd) = std::env::current_dir() {
                    if Path::new(&cwd).join(".git").exists() {
                        return Ok(cwd);
                    }
                }
                bail!("no repo given and cwd is not a git repo")
            }
        }
    }
}
