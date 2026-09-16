//! pheobe — a coding workhorse with no face.
//!
//! Task in, worktree branch + handoff report out. The loop stages, the task
//! schema, and the handoff report are the contract; the LLM behind the loop
//! is not. See DESIGN.md.

pub mod agent;
pub mod aging;
pub mod checkpoint;
pub mod fmt;
pub mod knowledge;
pub mod learn;
pub mod llm;
pub mod memory;
pub mod plan;
pub mod report;
pub mod sandbox;
pub mod structint;
pub mod task;
pub mod testparse;
pub mod tools;
pub mod verify;
pub mod worker;
pub mod worker_opencode;
pub mod worker_claude;
pub mod worker_codex;
pub mod worker_cursor;
pub mod worker_kimi;
pub mod worktree;
#[cfg(test)]
mod tests;
