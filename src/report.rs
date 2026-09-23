//! The handoff report — the stable contract every adopting harness consumes.
//! Semantics per runes discipline: one meaning per field, relations explicit.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffReport {
    pub ok: bool,
    pub task: String,
    /// Branch pheobe cooked on. Never a protected branch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub commits: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestEvidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Actions for the parent. Where uncertainty goes instead of pretending.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_steps: Vec<String>,
    /// Unverified assumptions / stale-knowledge claims (canon virtue 6-7).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doubts: Vec<String>,
    /// Intake or mid-run blockers. `blocked` runs report ok:false, never improvise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestEvidence {
    pub ran: String,
    pub passed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_excerpt: Option<String>,
    /// Structured parse of the runner output (PHEOBE-10): runner name,
    /// totals and failure records, when the output matched a known runner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parsed: Option<crate::testparse::TestReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub turns: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usd: Option<f64>,
}

impl HandoffReport {
    pub fn failure(task: &str, why: &str) -> Self {
        HandoffReport {
            ok: false,
            task: task.to_string(),
            branch: None,
            worktree: None,
            commits: vec![],
            tests: None,
            summary: None,
            next_steps: vec![],
            doubts: vec![],
            blocked: Some(why.to_string()),
            usage: None,
        }
    }
}

/// Emit the report on stdout as the run's last act.
/// Exit codes for `pheobe run` (PHEOBE-42). The JSON report is ALWAYS on
/// stdout; the code only tells a dispatcher which case it's in without
/// parsing:
/// - `0` — ran, `ok: true`
/// - `1` — ran, `ok: false` (done_when failed, blocked, budget/ttl, allowlist)
/// - `2` — never ran: intake or provisioning error (report carries it in `blocked`)
pub const EXIT_OK: i32 = 0;
pub const EXIT_NOT_OK: i32 = 1;
pub const EXIT_ERROR: i32 = 2;

/// Map a run result to (report, exit code). An error still produces a
/// report, so stdout is always one JSON object.
pub fn cli_outcome(task: &str, res: anyhow::Result<HandoffReport>) -> (HandoffReport, i32) {
    match res {
        Ok(rep) => {
            let code = if rep.ok { EXIT_OK } else { EXIT_NOT_OK };
            (rep, code)
        }
        Err(e) => (
            HandoffReport::failure(task, &format!("error: {e:#}")),
            EXIT_ERROR,
        ),
    }
}

pub fn emit(report: &HandoffReport) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(report)?);
    Ok(())
}
