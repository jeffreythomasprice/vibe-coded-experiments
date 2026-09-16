//! The `/ws` upgrade endpoint: accepts the socket, mints a `ConnectionId`, and runs the read loop
//! that turns each inbound `ClientEnvelope` into a `handler::handle` call plus whatever sends and
//! connections-table bookkeeping its result implies.

mod handler;
mod hub;

pub use hub::Hub;

use crate::access_codes::AccessCodeStore;
use crate::connections::ConnectionStore;
use crate::rooms::RoomStore;
use crate::routes::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::Response;
use futures_util::{SinkExt, StreamExt};
use handler::RoomTransition;
use shared::protocol::{ClientEnvelope, ConnectionId, ProtocolError, RequestId, ServerEnvelope, ServerMessage};
use std::time::Duration;
use tokio::sync::mpsc;

/// How long a freshly accepted socket has to send one message with a token that actually
/// validates, before it's dropped. The upgrade itself needs no token at all — a browser can't
/// attach headers to `new WebSocket()` — so an accepted-but-silent (or never-valid) socket would
/// otherwise be indistinguishable from a slow, legitimate client and sit open forever.
const AUTH_TIMEOUT: Duration = Duration::from_secs(15);

pub async fn upgrade<A, R, C>(ws: WebSocketUpgrade, State(state): State<AppState<A, R, C>>) -> Response
where
    A: AccessCodeStore,
    R: RoomStore,
    C: ConnectionStore,
{
    ws.on_upgrade(move |socket| run(socket, state))
}

fn envelope_message(reply_to: Option<RequestId>, message: ServerMessage) -> Message {
    let envelope = ServerEnvelope { reply_to, message };
    // Always encodable: every field here is a plain enum/struct of strings, numbers, and
    // already-serializable battle types — nothing with a non-string map key or a float to trip on
    // (see `shared::protocol::hash`'s old doc comment on exactly that hazard, back when this crate
    // still had one).
    Message::Text(serde_json::to_string(&envelope).expect("ServerEnvelope always encodes").into())
}

async fn run<A, R, C>(socket: WebSocket, state: AppState<A, R, C>)
where
    A: AccessCodeStore,
    R: RoomStore,
    C: ConnectionStore,
{
    let connection_id = ConnectionId(uuid::Uuid::new_v4().to_string());

    let (mut sink, mut stream) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Message>();
    state.hub.register(connection_id.clone(), tx.clone());

    // Owns the actual socket write half so the read loop below never blocks on a slow client --
    // it only ever hands a `Message` to this channel and moves on. Reading a `Close` frame off
    // `stream` (below) makes tungstenite queue its own answering `Close` on this same connection
    // internally, before this task ever sees it here -- confirmed live: an explicit second
    // `Message::Close` send of our own fails with "sending after closing is not allowed". What
    // *is* this task's job is `sink.close()` once there's nothing left to write: without it nothing
    // ever flushes that queued frame or shuts the transport down cleanly, and every disconnect
    // looks abnormal (code 1006) to the client even though the protocol-level handshake happened.
    let writer = tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            if sink.send(message).await.is_err() {
                break;
            }
        }
        let _ = sink.close().await;
    });

    // Recorded from the moment the socket is accepted, not from its first message -- a socket
    // that never sends anything valid before `AUTH_TIMEOUT` still occupied a connection for that
    // whole window, and this table exists precisely to track that (see `ConnectionStore::touch`'s
    // doc comment).
    let _ = state.connections.touch(&connection_id, "", None).await;

    let mut current_room: Option<String> = None;
    let mut access_key = String::new();
    let mut authenticated = false;
    let auth_deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;

    loop {
        let next = if authenticated {
            stream.next().await
        } else {
            match tokio::time::timeout_at(auth_deadline, stream.next()).await {
                Ok(next) => next,
                Err(_) => break,
            }
        };
        let Some(Ok(frame)) = next else { break };

        let text = match frame {
            Message::Text(text) => text,
            // The writer task (above) finishes the closing handshake once this loop ends.
            Message::Close(_) => break,
            // Axum answers a client `Ping` with a `Pong` automatically; neither carries an
            // application message, and this server never sends its own `Ping`, so a `Pong` here
            // would only ever be a browser oddity, not a real keepalive round trip.
            Message::Binary(_) | Message::Ping(_) | Message::Pong(_) => continue,
        };

        let Ok(envelope) = serde_json::from_str::<ClientEnvelope>(&text) else {
            tracing::debug!(connection = connection_id.0, "ignoring an undecodable message");
            continue;
        };
        let ClientEnvelope { id: request_id, token, message } = envelope;

        let outcome = {
            // Held for the whole handler call, not just whichever `RoomStore` calls happen to be
            // inside it — see `Hub::lock_rooms`'s doc comment.
            let _guard = state.hub.lock_rooms().await;
            handler::handle(&state.access_codes, &state.rooms, &state.sessions, &connection_id, current_room.as_deref(), &token, message).await
        };

        // A valid token proves this socket is a real client even if the specific action it asked
        // for was refused (a stale `Join` against a room that no longer exists, say) -- only an
        // outright bad/unknown token counts against the auth deadline.
        authenticated = authenticated || !matches!(outcome, Err(ProtocolError::Unauthorized));
        if !matches!(outcome, Err(ProtocolError::Unauthorized)) {
            access_key = token;
        }

        match outcome {
            Ok(handled) => {
                for (target, message) in handled.sends {
                    let reply_to = (target == connection_id).then_some(request_id);
                    state.hub.send(&target, envelope_message(reply_to, message));
                }
                match handled.room_transition {
                    Some(RoomTransition::Entered(room)) => current_room = Some(room),
                    Some(RoomTransition::Left) => current_room = None,
                    None => {}
                }
            }
            Err(error) => {
                // A room that's gone by the time this message was handled (expired, or somehow
                // already deleted) means whatever this connection thought it was in is stale --
                // drop that locally too, or every later message keeps failing the same way with
                // no path back to Solo.
                if error == ProtocolError::NoSuchRoom {
                    current_room = None;
                }
                state.hub.send(&connection_id, envelope_message(Some(request_id), ServerMessage::Error { error }));
            }
        }

        // Spawned rather than awaited: nothing else in this loop depends on it completing, and
        // waiting for it here would mean every message pays a full DynamoDB round trip before the
        // next one is even read, throttling a burst of moves from one client to that latency
        // regardless of how fast this server could otherwise process them.
        {
            let state = state.clone();
            let connection_id = connection_id.clone();
            let access_key = access_key.clone();
            let current_room = current_room.clone();
            tokio::spawn(async move {
                let _ = state.connections.touch(&connection_id, &access_key, current_room.as_deref()).await;
            });
        }
    }

    // A clean disconnect (browser tab closed, network drop, the auth deadline above) never sends
    // `Leave` -- clean up as though it had, so the rest of the room finds out immediately rather
    // than waiting out the TTL. Deliberately bypasses the normal token check: a connection whose
    // access code was revoked mid-session should still get to clean up after itself. Still takes
    // the room lock like every other mutation in this file (see `Hub::lock_rooms`'s doc comment):
    // without it, this could race a concurrent action from another member for the same room and
    // lose to a `RoomStoreError::VersionConflict`, silently leaving this connection stuck in
    // `room.members` until the TTL reaps it.
    if let Some(room_key) = &current_room {
        let _guard = state.hub.lock_rooms().await;
        if let Some(handled) = handler::disconnect(&state.rooms, &connection_id, room_key).await {
            for (target, message) in handled.sends {
                if target != connection_id {
                    state.hub.send(&target, envelope_message(None, message));
                }
            }
        }
    }

    state.hub.unregister(&connection_id);
    let _ = state.connections.delete(&connection_id).await;

    // Drops this function's own sender; the hub's clone is already gone via `unregister` above,
    // so this is the last one — `rx` sees the channel close, the writer task's loop ends, and it
    // runs `sink.close()` (see that task's own doc comment) before this function returns.
    drop(tx);
    let _ = writer.await;
}
