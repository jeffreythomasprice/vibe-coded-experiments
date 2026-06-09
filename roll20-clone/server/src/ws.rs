use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use shared::{ChatMessage, ClientMessage, ServerMessage};

use crate::state::AppState;

/// HTTP handler that upgrades the connection to a WebSocket.
pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let client_id = state.next_client_id();
    tracing::trace!(%client_id, "websocket connected");

    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.subscribe();

    // Greet the newly connected client with its id.
    let welcome = ServerMessage::Welcome {
        client_id: client_id.clone(),
    };
    if let Ok(txt) = serde_json::to_string(&welcome) {
        let _ = sender.send(Message::Text(txt.into())).await;
    }

    // Forward broadcast messages down to this client.
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            match serde_json::to_string(&msg) {
                Ok(txt) => {
                    if sender.send(Message::Text(txt.into())).await.is_err() {
                        break;
                    }
                }
                Err(e) => tracing::warn!(error = %e, "failed to serialize server message"),
            }
        }
    });

    // Read messages from this client and fan them out to everyone.
    let recv_state = state.clone();
    let recv_id = client_id.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    handle_text(&recv_state, &recv_id, text.as_str());
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // If either task ends (disconnect), abort the other.
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    tracing::trace!(%client_id, "websocket disconnected");
}

/// Decode an incoming client message and broadcast the resulting update.
fn handle_text(state: &AppState, client_id: &str, text: &str) {
    let msg: ClientMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!(error = %e, "ignoring malformed client message");
            return;
        }
    };
    tracing::trace!(%client_id, ?msg, "client message");

    match msg {
        ClientMessage::Chat { text } => {
            state.broadcast(ServerMessage::Chat(ChatMessage {
                from: client_id.to_string(),
                text,
            }));
        }
        ClientMessage::MoveToken(mv) => {
            state.broadcast(ServerMessage::TokenMoved(mv));
        }
        ClientMessage::Ping => {
            state.broadcast(ServerMessage::Pong);
        }
        ClientMessage::Join { room, name } => {
            tracing::trace!(%room, %name, "join (stub)");
            // TODO: real room membership + presence tracking.
            state.broadcast(ServerMessage::Presence {
                clients: vec![client_id.to_string()],
            });
        }
    }
}
