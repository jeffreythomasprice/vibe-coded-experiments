//! The engine's IPC server: a unix-domain socket where clients connect, say
//! hello, ping to stay alive, and ask for stats.
//!
//! One async task per connection reads requests and writes responses; a shared
//! [`Registry`] tracks who's connected; a reaper task kicks anyone who hasn't
//! sent a message in [`MAX_IDLE`]. The accept loop runs until the process is
//! killed.

use std::time::Duration;

use anyhow::{Context, Result};
use tokio::io::BufReader;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::oneshot;

use shared::config::Config;
use shared::protocol::{PROTOCOL_VERSION, Request, Response, read_message, write_message};

use crate::registry::{Registry, SharedRegistry};

/// Drop a client that hasn't sent any message in this long.
const MAX_IDLE: Duration = Duration::from_secs(120);

/// How often the reaper scans for idle clients.
const REAP_INTERVAL: Duration = Duration::from_secs(15);

/// This server's name, reported in the `Hello` response.
const SERVER_NAME: &str = concat!("munchkin-engine ", env!("CARGO_PKG_VERSION"));

/// Bind the socket and serve until the process is killed.
///
/// The caller must already hold the single-instance lock: that guarantee is
/// what makes it safe to remove a leftover socket file below (it can only be a
/// previous, crashed engine's, since no other engine can be running).
pub async fn run(config: &Config) -> Result<()> {
    let path = &config.socket_file;

    // A stale socket file from a crashed engine would make `bind` fail with
    // EADDRINUSE. We hold the lock, so nothing else is listening — remove it.
    match std::fs::remove_file(path) {
        Ok(()) => tracing::debug!(socket = %path.display(), "removed stale socket file"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            return Err(e).with_context(|| format!("removing stale socket {}", path.display()));
        }
    }

    let listener = UnixListener::bind(path)
        .with_context(|| format!("binding engine socket {}", path.display()))?;
    tracing::info!(socket = %path.display(), "listening for clients");

    let registry = Registry::new_shared();

    // Reaper: periodically kick clients that have gone quiet.
    tokio::spawn(reaper(registry.clone()));

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                tokio::spawn(handle_connection(stream, registry.clone()));
            }
            Err(e) => {
                // One bad accept shouldn't bring the whole server down.
                tracing::warn!(error = %e, "accept failed; continuing");
            }
        }
    }
}

/// Periodically scan the registry and signal idle clients to disconnect.
async fn reaper(registry: SharedRegistry) {
    let mut tick = tokio::time::interval(REAP_INTERVAL);
    loop {
        tick.tick().await;
        // Brief lock, dropped at the end of this statement; no `.await` inside.
        registry.lock().unwrap().reap(MAX_IDLE);
    }
}

/// Serve one client until it disconnects, errors, or is kicked.
async fn handle_connection(stream: UnixStream, registry: SharedRegistry) {
    let (kick_tx, mut kick_rx) = oneshot::channel();
    let id = registry.lock().unwrap().insert(kick_tx);
    tracing::info!(client = id, "client connected");

    let (read_half, mut write_half) = stream.into_split();
    let mut reader = BufReader::new(read_half);
    let mut buf = String::new();

    loop {
        tokio::select! {
            res = read_message::<_, Request>(&mut reader, &mut buf) => match res {
                Ok(Some(req)) => {
                    let resp = handle_request(req, id, &registry);
                    if let Err(e) = write_message(&mut write_half, &resp).await {
                        tracing::warn!(client = id, error = %e, "write failed; dropping client");
                        break;
                    }
                }
                Ok(None) => {
                    tracing::info!(client = id, "client disconnected");
                    break;
                }
                Err(e) => {
                    tracing::warn!(client = id, error = %e, "protocol error; dropping client");
                    break;
                }
            },
            _ = &mut kick_rx => {
                tracing::info!(
                    client = id,
                    "kicked for inactivity (no message in {}s)",
                    MAX_IDLE.as_secs()
                );
                break;
            }
        }
    }

    registry.lock().unwrap().remove(id);
}

/// Turn a request into its response, updating the client's registry entry.
///
/// Synchronous on purpose: it only takes the registry's blocking lock briefly
/// and never `.await`s, so no guard is ever held across an await point.
fn handle_request(req: Request, id: u64, registry: &SharedRegistry) -> Response {
    match req {
        Request::Hello { client, version } => {
            registry.lock().unwrap().touch(id, Some(&client));
            if version != PROTOCOL_VERSION {
                return Response::Error {
                    message: format!(
                        "protocol version mismatch: server {PROTOCOL_VERSION}, client {version}"
                    ),
                };
            }
            Response::Hello {
                server: SERVER_NAME.to_owned(),
                version: PROTOCOL_VERSION,
                assigned_id: id,
            }
        }
        Request::Ping => {
            registry.lock().unwrap().touch(id, None);
            Response::Pong
        }
        Request::Stats => {
            let clients = {
                let mut reg = registry.lock().unwrap();
                reg.touch(id, None);
                reg.snapshot()
            };
            Response::Stats { clients }
        }
    }
}
