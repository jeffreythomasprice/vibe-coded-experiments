//! Shared infrastructure and data structures for the Munchkin game.
//!
//! This crate holds everything that is useful to more than one client of the
//! game — today that's the [`engine`](../engine/index.html) and the
//! [`tui`](../tui/index.html), and in the future it might be a web client too.
//!
//! - [`config`] — locating, loading, and parsing the TOML config file.
//! - [`logging`] — process-wide `tracing` setup (shared append-only log file,
//!   per-line pid + mode tags, optional stderr mirror).
//! - [`model`] — the (currently stubbed) game data structures.
//! - [`protocol`] — the JSON request/response messages the engine and its
//!   clients exchange over the unix socket, plus their newline-delimited framing.
//! - [`client`] — a reusable tokio [`client::IpcClient`] for talking to the engine.

pub mod client;
pub mod config;
pub mod logging;
pub mod model;
pub mod protocol;
