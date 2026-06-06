//! The wire protocol between the engine (server) and its clients (the tui, and
//! eventually others), plus the framing used to put messages on a stream.
//!
//! Payloads are JSON. Each message is one JSON object on its own line
//! (newline-delimited JSON): `serde_json` emits compact output and escapes any
//! embedded newline, so `\n` is an unambiguous frame terminator and we don't
//! need a heavier length-prefixed codec.
//!
//! The protocol is deliberately small for now — a [`Request::Hello`] handshake,
//! a [`Request::Ping`] keepalive, and a [`Request::Stats`] query — but the
//! enums are tagged and use struct variants, so new messages and fields can be
//! added later without breaking older peers.

use anyhow::Result;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// Bumped whenever the wire format changes incompatibly. Exchanged in `Hello`
/// both ways so a mismatch can be reported instead of silently misbehaving.
pub const PROTOCOL_VERSION: u32 = 1;

/// A message from a client to the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    /// First message on a connection: introduces the client by name.
    Hello { client: String, version: u32 },
    /// Keepalive / liveness probe. Receipt of *any* message resets the client's
    /// idle timer, so a regular `Ping` is what keeps a connection from being
    /// reaped.
    Ping,
    /// Ask for server statistics, including the list of connected clients.
    Stats,
}

/// A message from the engine to a client.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    /// Reply to [`Request::Hello`]: identifies the server and the id it assigned
    /// to this connection.
    Hello {
        server: String,
        version: u32,
        assigned_id: u64,
    },
    /// Reply to [`Request::Ping`].
    Pong,
    /// Reply to [`Request::Stats`].
    Stats { clients: Vec<ClientInfo> },
    /// The request could not be handled (e.g. a protocol-version mismatch).
    Error { message: String },
}

/// One connected client, as reported in [`Response::Stats`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    /// Server-assigned connection id, unique for the engine's lifetime.
    pub id: u64,
    /// The name the client gave in its `Hello` (empty until it says hello).
    pub name: String,
    /// When the client connected, as Unix epoch milliseconds.
    pub connected_at_epoch_ms: u64,
    /// When the client last sent a message, as Unix epoch milliseconds.
    pub last_ping_epoch_ms: u64,
    /// How long since that last message, in milliseconds (server-computed).
    pub idle_ms: u64,
}

/// Serialise `msg` as a single JSON line and write it, flushing afterwards.
pub async fn write_message<W, T>(w: &mut W, msg: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    w.write_all(line.as_bytes()).await?;
    w.flush().await?;
    Ok(())
}

/// Read one JSON line from `r` and parse it into a `T`.
///
/// `buf` is cleared and reused as scratch across calls. Returns:
/// - `Ok(None)` on a clean EOF (the peer closed the connection at a frame
///   boundary),
/// - `Ok(Some(msg))` on a complete message,
/// - `Err(..)` on an IO or JSON-parse error.
///
/// `read_line` accumulates bytes until a newline, so a message split across
/// multiple reads is reassembled correctly — callers never see a half message.
pub async fn read_message<R, T>(r: &mut R, buf: &mut String) -> Result<Option<T>>
where
    R: AsyncBufRead + Unpin,
    T: DeserializeOwned,
{
    buf.clear();
    let n = r.read_line(buf).await?;
    if n == 0 {
        return Ok(None);
    }
    let msg = serde_json::from_str(buf.trim_end())?;
    Ok(Some(msg))
}
