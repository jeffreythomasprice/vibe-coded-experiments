//! Tauri commands. Thin: every command borrows `lib::service::Service` from
//! managed state and does nothing but call into it and flatten the error —
//! see `lib/src/lib.rs`'s module doc for why the actual logic lives in `lib`
//! rather than here.

pub mod agents;
pub mod conversations;
pub mod preferences;
