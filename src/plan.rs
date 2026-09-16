//! The tracked plan — `.pheobe/plan.json`. Informal (English) steps whose
//! formal counterparts are the code; say-it-twice, persisted for the parent.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub task: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    /// One meaning: the concrete change this step makes. Checkable state at the end.
    pub desc: String,
    /// todo | doing | done | blocked
    pub status: String,
    /// Unverified assumptions recorded here, never argued away (record-doubts).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doubts: Vec<String>,
}

pub fn path_in(worktree: &Path) -> PathBuf {
    worktree.join(".pheobe").join("plan.json")
}

pub fn load(worktree: &Path) -> Result<Option<Plan>> {
    let p = path_in(worktree);
    if !p.exists() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_str(&std::fs::read_to_string(p)?)?))
}

pub fn save(worktree: &Path, plan: &Plan) -> Result<()> {
    let dir = worktree.join(".pheobe");
    std::fs::create_dir_all(&dir)?;
    let _ = dir; // keep path_in authoritative
    std::fs::write(path_in(worktree), serde_json::to_string_pretty(plan)?)?;
    Ok(())
}
