//! The tui's connection to the engine.
//!
//! This module owns the *connection* concern only: it connects (failing fast,
//! non-zero, if the engine isn't up — this happens before the UI takes over the
//! terminal) and performs the [`Request::Hello`] handshake, then hands the live
//! [`IpcClient`] and the server's identity back to the caller. The render loop
//! and keepalive pinging live in [`crate::app`].

use anyhow::{Result, bail};

use shared::client::IpcClient;
use shared::config::Config;
use shared::protocol::{PROTOCOL_VERSION, Request, Response};

/// Who we're connected to, as reported by the engine's `Hello` reply.
#[derive(Debug, Clone)]
pub struct ServerIdentity {
    /// Human-readable server name.
    pub name: String,
    /// The protocol version the engine speaks.
    pub version: u32,
    /// The connection id the engine assigned us.
    pub assigned_id: u64,
}

/// Connect to the engine and complete the handshake.
///
/// Returns the live client plus the server's identity. Errors (and so exits the
/// process non-zero) if the engine isn't listening or rejects the handshake.
pub async fn connect(config: &Config) -> Result<(IpcClient, ServerIdentity)> {
    // Fail fast (and non-zero) if the engine isn't listening.
    let mut client = IpcClient::connect(&config.socket_file).await?;
    tracing::info!(socket = %config.socket_file.display(), "connected to engine");

    // Handshake.
    let identity = match client
        .request(Request::Hello {
            client: "tui".to_owned(),
            version: PROTOCOL_VERSION,
        })
        .await?
    {
        Response::Hello {
            server,
            version,
            assigned_id,
        } => {
            tracing::info!(%server, version, assigned_id, "handshake complete");
            ServerIdentity {
                name: server,
                version,
                assigned_id,
            }
        }
        Response::Error { message } => bail!("engine rejected hello: {message}"),
        other => bail!("unexpected handshake response: {other:?}"),
    };

    Ok((client, identity))
}
