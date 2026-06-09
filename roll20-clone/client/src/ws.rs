//! WebSocket connection to the server.
//!
//! This is a stub: it opens the connection, reports status into a Leptos
//! signal, and logs incoming [`ServerMessage`]s. Hook real app state up to the
//! read loop as features land.

use futures_util::StreamExt;
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::prelude::*;
use shared::ServerMessage;
use wasm_bindgen_futures::spawn_local;

/// The WebSocket URL, baked in at build time from `client/.env`.
pub const SERVER_WS_URL: &str = env!("SERVER_WS_URL");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnStatus {
    Connecting,
    Connected,
    Disconnected,
    Error,
}

impl ConnStatus {
    pub fn label(self) -> &'static str {
        match self {
            ConnStatus::Connecting => "connecting…",
            ConnStatus::Connected => "connected",
            ConnStatus::Disconnected => "disconnected",
            ConnStatus::Error => "error",
        }
    }
}

/// Open the WebSocket and drive its read loop, updating `status` as it changes.
pub fn connect(status: WriteSignal<ConnStatus>) {
    tracing::trace!(url = SERVER_WS_URL, "opening websocket");

    let ws = match WebSocket::open(SERVER_WS_URL) {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!(error = %e, "failed to open websocket");
            status.set(ConnStatus::Error);
            return;
        }
    };
    status.set(ConnStatus::Connected);

    let (_write, mut read) = ws.split();
    spawn_local(async move {
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => match serde_json::from_str::<ServerMessage>(&text) {
                    Ok(server_msg) => tracing::trace!(?server_msg, "received"),
                    Err(e) => tracing::warn!(error = %e, raw = %text, "undecodable message"),
                },
                Ok(Message::Bytes(b)) => tracing::trace!(len = b.len(), "received binary frame"),
                Err(e) => {
                    tracing::warn!(error = %e, "websocket error");
                    break;
                }
            }
        }
        tracing::trace!("websocket closed");
        status.set(ConnStatus::Disconnected);
    });
}
