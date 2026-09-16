//! pheobe learn — closed-loop learning, borrowed from exosphere's
//! memory-service `learning.rs` (itself inspired by NousResearch/hermes-agent).
//!
//! Free functions delegate to the active `MemoryStore` (selected by
//! `PHEOBE_MEMORY` at run start via `memory::init()`). The store can be
//! swapped without touching callers.

pub use crate::memory::{Nudge, Session};

pub fn init() {
    crate::memory::init();
}

pub fn enabled() -> bool {
    crate::memory::current().enabled()
}

pub fn begin_session(repo: &std::path::Path, task: &str) -> anyhow::Result<Option<Session>> {
    crate::memory::current().session_begin(repo, task)
}

pub fn end_session(s: Option<&Session>, outcome: &str, exit: i32) -> anyhow::Result<()> {
    crate::memory::current().session_end(s, outcome, exit)
}

pub fn log_event(session: &str, kind: &str, tool: Option<&str>, payload: Option<&str>) {
    crate::memory::current().log_event(session, kind, tool, payload);
}

pub fn store_nudge(repo: &str, text: &str, trigger_terms: Option<&str>) -> anyhow::Result<()> {
    crate::memory::current().store_nudge(repo, text, trigger_terms)
}

pub fn nudges_for(repo: &str) -> Vec<String> {
    crate::memory::current().nudges_for(repo)
}
