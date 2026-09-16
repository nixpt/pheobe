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
pub mod plan;
pub mod report;
pub mod task;
pub mod testparse;
pub mod tools;
pub mod verify;
pub mod worker;
pub mod worker_codex;
pub mod worktree;

#[cfg(test)]
mod tests;
