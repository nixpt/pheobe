//! pheobe — a coding workhorse with no face.
//!
//! Task in, worktree branch + handoff report out. The loop stages, the task
//! schema, and the handoff report are the contract; the LLM behind the loop
//! is not. See DESIGN.md.

pub mod knowledge;
pub mod learn;
pub mod plan;
pub mod report;
pub mod task;
pub mod verify;
pub mod worktree;

#[cfg(test)]
mod tests;
