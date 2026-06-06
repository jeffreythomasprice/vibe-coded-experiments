//! A small, reusable client for talking to the engine over its unix socket.
//!
//! [`IpcClient`] owns one connection and exposes a strictly request/response
//! API. Because a single stream correlates replies only by *order*, the
//! `&mut self` on [`IpcClient::request`] is what prevents two requests being in
//! flight at once. If concurrent in-flight requests are ever needed, add a
//! request-id to the [`crate::protocol`] messages and a correlation map here.

use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

use crate::protocol::{Request, Response, read_message, write_message};

/// A connected client of the engine.
pub struct IpcClient {
    reader: BufReader<OwnedReadHalf>,
    writer: OwnedWriteHalf,
    buf: String,
}

impl IpcClient {
    /// Connect to the engine's unix socket at `path`.
    pub async fn connect(path: &Path) -> Result<Self> {
        let stream = UnixStream::connect(path)
            .await
            .with_context(|| format!("connecting to engine socket {}", path.display()))?;
        let (read_half, writer) = stream.into_split();
        Ok(Self {
            reader: BufReader::new(read_half),
            writer,
            buf: String::new(),
        })
    }

    /// Send a request without waiting for a reply.
    pub async fn send(&mut self, req: &Request) -> Result<()> {
        write_message(&mut self.writer, req).await
    }

    /// Read the next response. Errors if the engine closed the connection (e.g.
    /// it kicked us for inactivity).
    pub async fn recv(&mut self) -> Result<Response> {
        match read_message(&mut self.reader, &mut self.buf).await? {
            Some(resp) => Ok(resp),
            None => bail!("engine closed the connection"),
        }
    }

    /// Send a request and wait for its response.
    pub async fn request(&mut self, req: Request) -> Result<Response> {
        self.send(&req).await?;
        self.recv().await
    }
}
