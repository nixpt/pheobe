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
    /// Optional dependency edges on earlier step indices (0-based). A step
    /// with unsatisfied deps must not start. Future: a parent fanning out
    /// several pheobe nodes can consume this plan as a sub-DAG (Graph
    /// Harness vocabulary: immutable per run, wave-ordered by the parent).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<u32>,
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

/// PHEOBE-10 depends_on validation: indices in range, no self-reference, no
/// cycles (bounded DFS, polydex idiom — surfaces `cycle_detected`), and a
/// done step's predecessors must all be done (the refusal lists the blockers).
pub fn validate_steps(steps: &[Step]) -> std::result::Result<(), String> {
    let n = steps.len();
    for (i, s) in steps.iter().enumerate() {
        for &d in &s.depends_on {
            if d as usize >= n {
                return Err(format!(
                    "depends_on index {d} out of range (plan has {n} steps) — step {i} ('{}')",
                    s.desc
                ));
            }
            if d as usize == i {
                return Err(format!("step {i} ('{}') depends on itself", s.desc));
            }
        }
    }
    if let Some(cyc) = find_cycle(steps) {
        let named: Vec<String> = cyc.iter().map(|&i| format!("{i} ('{}')", steps[i].desc)).collect();
        return Err(format!("cycle_detected: [{}]", named.join(" -> ")));
    }
    for (i, s) in steps.iter().enumerate() {
        if s.status != "done" {
            continue;
        }
        let blocking: Vec<String> = s
            .depends_on
            .iter()
            .map(|&d| d as usize)
            .filter(|&d| steps[d].status != "done")
            .map(|d| format!("{} ('{}')", d, steps[d].desc))
            .collect();
        if !blocking.is_empty() {
            return Err(format!(
                "step {i} ('{}') cannot be marked done — blocking predecessors not done: [{}]",
                s.desc,
                blocking.join(", ")
            ));
        }
    }
    Ok(())
}

/// Bounded DFS over the depends_on graph; returns the cycle path (closed) on
/// the first cycle found. Bounded because a path longer than the node count
/// must have revisited a node and been rejected already.
fn find_cycle(steps: &[Step]) -> Option<Vec<usize>> {
    for start in 0..steps.len() {
        let mut stack = vec![(start, vec![start])];
        while let Some((node, path)) = stack.pop() {
            for &d in &steps[node].depends_on {
                let d = d as usize;
                if let Some(pos) = path.iter().position(|&x| x == d) {
                    let mut cyc: Vec<usize> = path[pos..].to_vec();
                    cyc.push(d);
                    return Some(cyc);
                }
                let mut p = path.clone();
                p.push(d);
                stack.push((d, p));
            }
        }
    }
    None
}

pub fn save(worktree: &Path, plan: &Plan) -> Result<()> {
    let dir = worktree.join(".pheobe");
    std::fs::create_dir_all(&dir)?;
    let _ = dir; // keep path_in authoritative
    std::fs::write(path_in(worktree), serde_json::to_string_pretty(plan)?)?;
    Ok(())
}
