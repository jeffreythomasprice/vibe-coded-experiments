use std::sync::{Arc, Mutex};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use shared::{ChatMessage, ClientMessage, MapId, ServerMessage};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;

use crate::state::AppState;

/// Which map (if any) a connection is following. Shared between a connection's
/// receive task (which sets it on `FollowMap`) and its send task (which uses it
/// to decide whether to forward a `MapUpdated` frame).
type Followed = Arc<Mutex<Option<MapId>>>;

/// A connection's private outbound channel, for messages addressed to just this
/// client (the welcome greeting, follow acks) rather than broadcast to all.
type PersonalTx = mpsc::UnboundedSender<ServerMessage>;

/// HTTP handler that upgrades the connection to a WebSocket.
pub async fn handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> Response {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let client_id = state.next_client_id();
    tracing::trace!(%client_id, "websocket connected");

    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.subscribe();

    let followed: Followed = Arc::new(Mutex::new(None));
    let (personal_tx, mut personal_rx) = mpsc::unbounded_channel::<ServerMessage>();

    // Greet the newly connected client with its id (private to this client).
    let _ = personal_tx.send(ServerMessage::Welcome {
        client_id: client_id.clone(),
    });

    // Forward broadcast + personal messages down to this client. `MapUpdated`
    // frames are filtered to the map this connection is following.
    let send_followed = followed.clone();
    let mut send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                broadcast = rx.recv() => {
                    let msg = match broadcast {
                        Ok(m) => m,
                        Err(RecvError::Lagged(_)) => continue,
                        Err(RecvError::Closed) => break,
                    };
                    let forward = match &msg {
                        ServerMessage::MapUpdated { map } => {
                            send_followed.lock().unwrap().as_deref() == Some(map.id.as_str())
                        }
                        _ => true,
                    };
                    if forward && send_text(&mut sender, &msg).await.is_err() {
                        break;
                    }
                }
                personal = personal_rx.recv() => {
                    match personal {
                        Some(msg) => {
                            if send_text(&mut sender, &msg).await.is_err() {
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    // Read messages from this client.
    let recv_state = state.clone();
    let recv_id = client_id.clone();
    let recv_followed = followed.clone();
    let recv_personal = personal_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                Message::Text(text) => {
                    handle_text(
                        &recv_state,
                        &recv_id,
                        text.as_str(),
                        &recv_followed,
                        &recv_personal,
                    );
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

/// Serialize and send a single message; `Err(())` means the socket is gone.
async fn send_text(
    sender: &mut SplitSink<WebSocket, Message>,
    msg: &ServerMessage,
) -> Result<(), ()> {
    match serde_json::to_string(msg) {
        Ok(txt) => sender.send(Message::Text(txt.into())).await.map_err(|_| ()),
        Err(e) => {
            tracing::warn!(error = %e, "failed to serialize server message");
            Ok(())
        }
    }
}

/// Decode an incoming client message and act on it.
fn handle_text(
    state: &AppState,
    client_id: &str,
    text: &str,
    followed: &Followed,
    personal: &PersonalTx,
) {
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
        ClientMessage::FollowMap { map_id } => {
            // A client follows exactly one map; following a new one replaces it.
            *followed.lock().unwrap() = Some(map_id.clone());
            let _ = personal.send(ServerMessage::FollowingMap { map_id });
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
