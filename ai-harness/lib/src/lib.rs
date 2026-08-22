//! Server-side tools: everything the Tauri backend needs that isn't Tauri itself.
//!
//! Kept separate from the `server` crate so it can be unit-tested without
//! booting an application window.

pub mod config;
pub mod llm;
pub mod logging;

pub fn build_greeting(name: &str) -> String {
    tracing::trace!(name, "building greeting");
    format!("Hello, {name}! This greeting was built on the server, in the `lib` crate.")
}
