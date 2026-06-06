//! The tui's connection to the engine.
//!
//! The UI itself is still a stub, so this just exercises the protocol: connect
//! (failing fast if the engine isn't up), say hello, then keep the connection
//! alive by pinging every [`PING_INTERVAL`] and occasionally asking for server
//! stats — logging everything. It runs until the process is killed or the
//! engine drops us (e.g. kicks us for inactivity), in which case it returns an
//! error so the process exits non-zero.

use std::time::Duration;

use anyhow::{Result, bail};

use shared::client::IpcClient;
use shared::config::Config;
use shared::protocol::{PROTOCOL_VERSION, Request, Response};

/// How often to ping the engine to stay alive.
const PING_INTERVAL: Duration = Duration::from_secs(30);

/// Ask for stats every Nth ping.
const STATS_EVERY: u64 = 4;

/// Connect to the engine and run the keepalive loop forever.
pub async fn run(config: &Config) -> Result<()> {
    // Fail fast (and non-zero) if the engine isn't listening.
    let mut client = IpcClient::connect(&config.socket_file).await?;
    tracing::info!(socket = %config.socket_file.display(), "connected to engine");

    // Handshake.
    match client
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
        } => tracing::info!(%server, version, assigned_id, "handshake complete"),
        Response::Error { message } => bail!("engine rejected hello: {message}"),
        other => bail!("unexpected handshake response: {other:?}"),
    }

    let mut ping = tokio::time::interval(PING_INTERVAL);
    let mut n: u64 = 0;
    loop {
        ping.tick().await;
        n += 1;

        // A failed request means the engine closed the connection (e.g. kicked
        // us); propagate so the process exits non-zero.
        match client.request(Request::Ping).await? {
            Response::Pong => tracing::debug!("pong"),
            other => tracing::warn!("unexpected ping response: {other:?}"),
        }

        if n % STATS_EVERY == 0 {
            match client.request(Request::Stats).await? {
                Response::Stats { clients } => {
                    tracing::info!(count = clients.len(), ?clients, "server stats");
                }
                other => tracing::warn!("unexpected stats response: {other:?}"),
            }
        }
    }
}
