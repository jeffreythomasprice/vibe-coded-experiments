//! Server-side tools: everything the Tauri backend needs that isn't Tauri itself.
//!
//! Kept separate from the `server` crate so it can be unit-tested without
//! booting an application window.

pub mod agent;
pub mod cache;
pub mod catalog;
pub mod config;
pub mod llm;
pub mod logging;
