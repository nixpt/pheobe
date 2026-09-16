//! pheobe learn — closed-loop learning, borrowed from exosphere's
//! memory-service `learning.rs` (itself inspired by NousResearch/hermes-agent).
//!
//! The tables become JSONL append-only stores, keeping the default build
//! dependency-free (rusqlite arrives only if the learning DB ever grows into
//! the shared joker one via config). Four records mirror the upstream tables:
//!
//! - sessions  — lifecycle (start/end/outcome)   ← learning_sessions
//! - events    — passive capture of each loop stage/tool call ← session_events
//! - nudges    — resurfaced gotchas, scoped per repo, with trigger terms ← nudge_items
//! - candidates— procedures pending approval (skill_candidates) — v0.2
//!
//! Post-run REVIEW (session_events → lessons → nudges) is the one piece that
//! needs a model; in self mode it's a second cheap pass after the loop, in
//! host mode the *parent* reviews and stores (or the host's own memory does).
//!
//! Disabled unless `PHEOBE_LEARN=1` or the learning dir already exists — a
//! scoped worker defaults to memoryless; the store is an operator choice.
//! Host mode overrides: `PHEOBE_MEMORY=none|local|host` (default local).

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub repo: String,
    pub task: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Nudge {
    pub scope_type: String, // "repo" | "global"
    pub scope_value: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_terms: Option<String>,
    #[serde(default)]
    pub shown: u32,
    #[serde(default)]
    pub created_at: String,
}

fn learning_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("PHEOBE_LEARNING_DIR") {
        return Some(PathBuf::from(d));
    }
    std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".pheobe").join("learning"))
}

/// Enabled iff `PHEOBE_LEARN=1` or the learning dir already exists on disk.
pub fn enabled() -> bool {
    if std::env::var("PHEOBE_LEARN").ok().as_deref() == Some("1") {
        return true;
    }
    learning_dir().is_some_and(|d| d.is_dir())
}

fn append_jsonl(file: &Path, line: &str) -> Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(file)?;
    writeln!(f, "{line}")?;
    Ok(())
}

pub fn begin_session(repo: &Path, task: &str) -> Result<Option<Session>> {
    if !enabled() {
        return Ok(None);
    }
    let s = Session {
        id: format!("s-{}", now_unix()),
        repo: repo.display().to_string(),
        task: task.to_string(),
        started_at: now_iso(),
        ended_at: None,
        outcome: None,
        exit_code: None,
    };
    if let Some(dir) = learning_dir() {
        append_jsonl(&dir.join("sessions.jsonl"), &serde_json::to_string(&s)?)?;
    }
    Ok(Some(s))
}

pub fn end_session(s: Option<&Session>, outcome: &str, exit: i32) -> Result<()> {
    let Some(s) = s else { return Ok(()) };
    if !enabled() {
        return Ok(());
    }
    let closed = Session {
        id: s.id.clone(),
        repo: s.repo.clone(),
        task: s.task.clone(),
        started_at: s.started_at.clone(),
        ended_at: Some(now_iso()),
        outcome: Some(outcome.to_string()),
        exit_code: Some(exit),
    };
    if let Some(dir) = learning_dir() {
        append_jsonl(&dir.join("sessions.jsonl"), &serde_json::to_string(&closed)?)?;
    }
    Ok(())
}

/// Passive event capture — one line per loop stage / tool call.
pub fn log_event(session: &str, kind: &str, tool: Option<&str>, payload: Option<&str>) {
    if !enabled() {
        return;
    }
    let ev = serde_json::json!({
        "session": session, "ts": now_iso(), "kind": kind,
        "tool": tool, "payload": payload,
    });
    if let Some(dir) = learning_dir() {
        let _ = append_jsonl(&dir.join("events.jsonl"), &ev.to_string());
    }
}

/// Record a lesson as a resurfaced nudge, scoped to a repo.
pub fn store_nudge(repo: &str, text: &str, trigger_terms: Option<&str>) -> Result<()> {
    let n = Nudge {
        scope_type: "repo".into(),
        scope_value: repo.to_string(),
        text: text.to_string(),
        trigger_terms: trigger_terms.map(|t| t.to_string()),
        shown: 0,
        created_at: now_iso(),
    };
    if let Some(dir) = learning_dir() {
        append_jsonl(&dir.join("nudges.jsonl"), &serde_json::to_string(&n)?)?;
    }
    Ok(())
}

/// Nudges for a repo — the orient-stage injection. Lines are not deduplicated:
/// repetition IS the salience signal (upstream increments `shown` instead; the
/// report's orient notes count repeats). Append-only upstream, too.
pub fn nudges_for(repo: &str) -> Vec<String> {
    if !enabled() {
        return vec![];
    }
    let Some(dir) = learning_dir() else { return vec![] };
    let Ok(txt) = std::fs::read_to_string(dir.join("nudges.jsonl")) else {
        return vec![];
    };
    txt.lines()
        .filter_map(|l| serde_json::from_str::<Nudge>(l).ok())
        .filter(|n| n.scope_type == "repo" && n.scope_value == repo)
        .map(|n| n.text)
        .collect()
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0) as i64;
    let days = secs / 86400;
    let rem = secs % 86400;
    // civil date from days (2026-epoch approximation: exact for now, fine for logs)
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", (rem / 3600), (rem % 3600) / 60, rem % 60)
}

/// Howard Hinnant's civil_from_days, stdlib-only.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (y + if m <= 2 { 1 } else { 0 }, m, d)
}
