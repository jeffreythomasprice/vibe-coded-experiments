//! Types shared across the IPC boundary.
//!
//! This crate is compiled for **both** the native backend and the wasm
//! frontend, so nothing here may touch `std::fs`, threads, or the clock.
//! `tracing` the facade is fine (it is a no-op without a subscriber);
//! subscribers are not.

use serde::{Deserialize, Serialize};

pub mod agent;
pub mod llm;

/// Severity of a forwarded client log event, mirroring `tracing::Level`.
///
/// `tracing::Level` is neither `Serialize` nor `Deserialize`, so the IPC
/// boundary carries this instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl From<&tracing::Level> for LogLevel {
    fn from(level: &tracing::Level) -> Self {
        match *level {
            tracing::Level::TRACE => LogLevel::Trace,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
        }
    }
}

/// A log event produced in the wasm frontend and forwarded to the backend, so
/// that client and server logs land in the same rotated files.
///
/// The frontend cannot write files, and `tracing`'s macros require *static*
/// metadata — so the backend re-emits these under a fixed `client` target with
/// the real origin carried in these fields. See `lib::logging::log_client_record`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientLogRecord {
    /// ISO8601 with millisecond precision, stamped in the browser at the moment
    /// the event fired. Forwarding is async, so this — not the backend's own
    /// timestamp on the log line — is the authoritative ordering for client events.
    pub timestamp: String,
    pub level: LogLevel,
    /// The client-side module path, e.g. `client::app`.
    pub target: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    pub message: String,
}
