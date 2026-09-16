//! verify — the mechanical exit gate (borrowed from mayfly's done_when +
//! jokersquad's scan gate). A run ends here or it reports failure.

use crate::report::TestEvidence;
use crate::task::{DoneWhen, Task};
use anyhow::Result;
use std::path::Path;
use std::process::Command;

/// Run the done_when gate. Returns test evidence (raw excerpt + structured
/// parse when the output matched a known runner).
pub fn run_done_when(task: &Task, cwd: &Path) -> Result<TestEvidence> {
    match &task.done_when {
        DoneWhen::Command { run, expect_exit } => {
            let out = Command::new("sh")
                .args(["-c", run])
                .current_dir(cwd)
                .output()?;
            let passed = out.status.code().unwrap_or(-1) == *expect_exit;
            let combined = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            let excerpt: String = combined
                .lines()
                .rev()
                .take(20)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n");
            let parsed = crate::testparse::parse(&combined);
            Ok(TestEvidence {
                ran: run.clone(),
                passed,
                output_excerpt: Some(excerpt),
                parsed,
            })
        }
    }
}
