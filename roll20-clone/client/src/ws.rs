//! WebSocket connection to the server.
//!
//! This is a stub: it opens the connection, reports status into a Leptos
//! signal, and logs incoming [`ServerMessage`]s. Hook real app state up to the
//! read loop as features land.

use futures_util::{SinkExt, StreamExt};
use gloo_net::websocket::{futures::WebSocket, Message};
use leptos::prelude::*;
use shared::{ClientMessage, Map, ServerMessage};
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

/// Open a dedicated WebSocket, follow `map_id`, and invoke `on_map` whenever the
/// server pushes an update for that map. The connection lives until the returned
/// future's task ends (i.e. the socket closes).
pub fn follow_map(map_id: String, on_map: impl Fn(Map) + 'static) {
    spawn_local(async move {
        let ws = match WebSocket::open(SERVER_WS_URL) {
            Ok(ws) => ws,
            Err(e) => {
                tracing::error!(error = %e, "failed to open follow websocket");
                return;
            }
        };
        let (mut write, mut read) = ws.split();

        // Register interest in this map.
        match serde_json::to_string(&ClientMessage::FollowMap {
            map_id: map_id.clone(),
        }) {
            Ok(txt) => {
                if let Err(e) = write.send(Message::Text(txt)).await {
                    tracing::error!(error = %e, "failed to send follow_map");
                    return;
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to serialize follow_map");
                return;
            }
        }

        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => match serde_json::from_str::<ServerMessage>(&text) {
                    Ok(ServerMessage::MapUpdated { map }) if map.id == map_id => on_map(map),
                    Ok(_) => {}
                    Err(e) => tracing::warn!(error = %e, raw = %text, "undecodable message"),
                },
                Ok(Message::Bytes(_)) => {}
                Err(e) => {
                    tracing::warn!(error = %e, "follow websocket error");
                    break;
                }
            }
        }
        tracing::trace!(%map_id, "follow websocket closed");
    });
}
