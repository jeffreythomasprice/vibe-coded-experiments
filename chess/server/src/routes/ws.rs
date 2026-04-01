use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::AppState;

#[derive(Deserialize)]
pub struct WsParams {
    pub token: Option<String>,
}

pub async fn game_ws(
    ws: WebSocketUpgrade,
    Path(game_id): Path<Uuid>,
    Query(params): Query<WsParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    // Validate JWT from query param
    if let Some(token) = &params.token {
        let token_data = jsonwebtoken::decode::<serde_json::Value>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(state.jwt_secret.as_bytes()),
            &jsonwebtoken::Validation::default(),
        );
        if token_data.is_err() {
            return axum::http::StatusCode::UNAUTHORIZED.into_response();
        }
    } else {
        return axum::http::StatusCode::UNAUTHORIZED.into_response();
    }

    ws.on_upgrade(move |socket| handle_socket(socket, game_id, state))
}

async fn handle_socket(mut socket: WebSocket, game_id: Uuid, state: AppState) {
    let mut rx = state.ai_manager.subscribe();

    loop {
        tokio::select! {
            // Forward matching game events to the client
            event = rx.recv() => {
                match event {
                    Ok(event) => {
                        let event_game_id = match &event {
                            crate::ai::manager::GameEvent::AiThinkingStarted { game_id } => *game_id,
                            crate::ai::manager::GameEvent::MoveMade { game_id, .. } => *game_id,
                            crate::ai::manager::GameEvent::AiThinkingCompleted { game_id } => *game_id,
                            crate::ai::manager::GameEvent::AiProgress { game_id, .. } => *game_id,
                        };

                        if event_game_id != game_id {
                            continue;
                        }

                        let json = match serde_json::to_string(&event) {
                            Ok(j) => j,
                            Err(_) => continue,
                        };

                        if socket.send(Message::Text(json.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(game_id = %game_id, lagged = n, "WebSocket client lagged");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        break;
                    }
                }
            }
            // Handle client messages (or disconnection)
            msg = socket.recv() => {
                match msg {
                    Some(Ok(_)) => {
                        // Ignore client messages for now
                    }
                    _ => break,
                }
            }
        }
    }
}
