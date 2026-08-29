//! Serializable agent types: what an agent is ([`AgentSpec`]), the result of
//! asking it for its next turn ([`AgentTurn`], [`TurnStop`]), and the events a
//! streaming turn emits ([`AgentEvent`]).
//!
//! These types cross the Tauri IPC boundary the same way `shared::llm` does —
//! see this crate's module doc for the no-`std::fs`/threads/clock constraint
//! that applies here too. `lib::agent` builds and runs an actual `Agent` from
//! an [`AgentSpec`]; nothing in this module executes anything.

pub mod config;
pub mod event;
pub mod spec;
pub mod turn;

pub use config::{AgentConfig, AgentConfigInput};
pub use event::AgentEvent;
pub use spec::{AgentSpec, Approval, ToolSpec};
pub use turn::{AgentTurn, DecidedBy, Decision, ToolApprovalRequest, ToolDecision, ToolDecisionRecord, TurnStop};
