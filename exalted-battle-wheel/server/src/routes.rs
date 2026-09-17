use crate::access_codes::{AccessCode, AccessCodeStore};
use crate::auth::{require_access_code, require_admin, Caller};
use crate::connections::ConnectionStore;
use crate::error::ApiError;
use crate::rooms::{RoomQuery, RoomStore, DEFAULT_ROOM_PAGE, MAX_ROOM_PAGE};
use crate::sessions::Sessions;
use crate::wire_json::WireJson;
use crate::ws::{self, Hub};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get};
use axum::{middleware, Json, Router};
use shared::access::{AccessCodeList, CreateAccessCode, UpdateAccessCode};
use shared::protocol::{room_key, LeaveReason, ServerMessage};
use shared::rooms::RoomList;

#[derive(Clone)]
pub struct AppState<A, R, C> {
    pub access_codes: A,
    pub rooms: R,
    pub connections: C,
    pub hub: Hub,
    pub sessions: Sessions,
}

pub fn router<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(state: AppState<A, R, C>) -> Router {
    // `/rooms` moved in here alongside `/access-codes`: a room name plus any access code is
    // effectively a join credential (see `client::link::invite_url`), so leaving the room list
    // itself readable by every code would make "admin-only" purely cosmetic for anyone willing to
    // brute-force room names against it.
    let admin = Router::new()
        .route("/access-codes", get(list_access_codes::<A, R, C>).post(create_access_code::<A, R, C>))
        .route(
            "/access-codes/{access_key}",
            get(read_access_code::<A, R, C>).put(update_access_code::<A, R, C>).delete(delete_access_code::<A, R, C>),
        )
        .route("/rooms", get(list_rooms::<A, R, C>))
        .route("/rooms/{room_name}", delete(delete_room::<A, R, C>))
        .route_layer(middleware::from_fn(require_admin));

    // `/auth/me` and `/access-codes/{access_key}` are siblings in one matchit tree, which always
    // prefers a static segment over a parameter -- but every route naming the key must spell it
    // `{access_key}`, since matchit treats two different parameter names at the same position as a
    // routing conflict.
    let authenticated = Router::new()
        .route("/auth/me", get(my_access_code))
        .merge(admin)
        .route_layer(middleware::from_fn_with_state(state.clone(), require_access_code::<A, R, C>));

    // `route_layer`, not `layer`: it skips the fallback, so an unauthenticated request to an
    // unknown path stays a 404 instead of becoming a 401. `/health` and `/ws` are added to the
    // outer router afterward, so neither is ever wrapped by `require_access_code` -- `/health`
    // for the pod's liveness/readiness probes, `/ws` because a browser can't attach an
    // `Authorization` header to `new WebSocket()`; every websocket message carries its own token
    // instead (see `ws`'s own doc comment).
    Router::new()
        .route("/health", get(health))
        .route("/ws", get(ws::upgrade::<A, R, C>))
        .merge(authenticated)
        .fallback(not_found)
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn not_found() -> ApiError {
    ApiError::NotFound
}

async fn my_access_code(Caller(code): Caller) -> Json<AccessCode> {
    Json(code)
}

/// `GET /rooms`'s raw query string -- not a wire DTO (nothing about it crosses a websocket or gets
/// persisted), so this skips `WireJson`/`shared::validate` and goes through axum's own `Query`
/// extractor instead.
#[derive(Debug, serde::Deserialize)]
struct ListRoomsParams {
    q: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
}

/// Normalizes one page request: blank/missing search means everything, and `limit` is clamped
/// into `1..=MAX_ROOM_PAGE` regardless of what was asked for, so nothing downstream (in particular
/// `RoomStore::list`'s own callers) has to trust a client-supplied page size.
fn room_query(params: ListRoomsParams) -> RoomQuery {
    let limit = params.limit.unwrap_or(DEFAULT_ROOM_PAGE).clamp(1, MAX_ROOM_PAGE);
    RoomQuery { search: params.q.unwrap_or_default(), limit, after: params.cursor }
}

async fn list_rooms<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(
    State(state): State<AppState<A, R, C>>,
    Query(params): Query<ListRoomsParams>,
) -> Result<Json<RoomList>, ApiError> {
    let page = state.rooms.list(&room_query(params)).await?;
    Ok(Json(RoomList { rooms: page.rooms, next_cursor: page.next }))
}

/// Deletes a room outright and disconnects everyone in it, regardless of who's currently
/// connected or what they were doing -- an admin closing a room someone else is using. Takes the
/// same process-wide room lock every other room mutation does (see `ws::hub::Hub::lock_rooms`'s
/// doc comment) so this can never race a member's own in-flight action against the room it's
/// about to remove. Deletes before notifying: the room is gone from `RoomStore` before anyone is
/// told to leave it, so a `Join` racing the notification can't slip back into a room that's about
/// to disappear underneath it.
async fn delete_room<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(
    State(state): State<AppState<A, R, C>>,
    Path(room_name): Path<String>,
) -> Result<StatusCode, ApiError> {
    let key = room_key(&room_name).map_err(|_| ApiError::NotFound)?;

    let guard = state.hub.lock_rooms().await;
    let Some(room) = state.rooms.get(&key).await? else { return Err(ApiError::NotFound) };
    state.rooms.delete(&key).await?;
    drop(guard);

    let targets = room.members.iter().map(|member| member.connection_id.clone());
    ws::notify(&state.hub, targets, ServerMessage::Left { reason: LeaveReason::RoomClosed });
    Ok(StatusCode::NO_CONTENT)
}

async fn list_access_codes<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(
    State(state): State<AppState<A, R, C>>,
) -> Result<Json<AccessCodeList>, ApiError> {
    let codes = state.access_codes.list().await?;
    Ok(Json(AccessCodeList { codes }))
}

async fn create_access_code<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(
    State(state): State<AppState<A, R, C>>,
    WireJson(body): WireJson<CreateAccessCode>,
) -> Result<(StatusCode, Json<AccessCode>), ApiError> {
    let access_key = body.access_key.as_ref().map(|key| key.trim()).filter(|key| !key.is_empty());
    let code = state.access_codes.create(access_key, body.is_admin).await?;
    Ok((StatusCode::CREATED, Json(code)))
}

async fn read_access_code<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(
    State(state): State<AppState<A, R, C>>,
    Path(access_key): Path<String>,
) -> Result<Json<AccessCode>, ApiError> {
    let code = state.access_codes.get(&access_key).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(code))
}

async fn update_access_code<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(
    Caller(caller): Caller,
    State(state): State<AppState<A, R, C>>,
    Path(access_key): Path<String>,
    WireJson(body): WireJson<UpdateAccessCode>,
) -> Result<Json<AccessCode>, ApiError> {
    // An admin can't demote or delete themselves through this API -- without this, the only way
    // back from locking out the last admin is the AWS console or CLI.
    if *caller.access_key == access_key {
        return Err(ApiError::SelfModification);
    }
    let code = state.access_codes.update(&access_key, body.is_admin).await?;
    Ok(Json(code))
}

async fn delete_access_code<A: AccessCodeStore, R: RoomStore, C: ConnectionStore>(
    Caller(caller): Caller,
    State(state): State<AppState<A, R, C>>,
    Path(access_key): Path<String>,
) -> Result<StatusCode, ApiError> {
    if *caller.access_key == access_key {
        return Err(ApiError::SelfModification);
    }
    state.access_codes.delete(&access_key).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_codes::MemoryAccessCodeStore;
    use crate::connections::MemoryConnectionStore;
    use crate::rooms::{MemoryRoomStore, Room, RoomId, RoomMember, ROOM_TTL};
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use shared::battle::BattleLog;
    use shared::protocol::ConnectionId;
    use time::OffsetDateTime;
    use tower::ServiceExt;

    /// Builds everything a test needs, including the room store and hub that `app()` (below)
    /// throws away -- kept separate rather than changing `app()`'s own return type, so the
    /// existing access-code tests (which only ever need the first two) don't all have to grow an
    /// extra `..` to keep compiling.
    fn full_app() -> (Router, MemoryAccessCodeStore, MemoryRoomStore, Hub) {
        let access_codes = MemoryAccessCodeStore::default();
        let rooms = MemoryRoomStore::default();
        let hub = Hub::default();
        let state = AppState {
            access_codes: access_codes.clone(),
            rooms: rooms.clone(),
            connections: MemoryConnectionStore::default(),
            hub: hub.clone(),
            sessions: Sessions::new("test-secret"),
        };
        (router(state), access_codes, rooms, hub)
    }

    fn app() -> (Router, MemoryAccessCodeStore) {
        let (app, access_codes, _rooms, _hub) = full_app();
        (app, access_codes)
    }

    /// A minimal live room under `key`, with no members -- callers push their own via
    /// `RoomMember` when a test needs one.
    fn sample_room(key: &str) -> Room {
        let now = OffsetDateTime::now_utc();
        Room {
            id: RoomId(format!("room-{key}")),
            room_key: key.to_string(),
            display_name: key.to_string(),
            version: 1,
            log: BattleLog::new(),
            everyone_writes: true,
            members: Vec::new(),
            updated_at: now,
            expires_at: now + ROOM_TTL,
        }
    }

    async fn request(app: &Router, method: &str, path: &str, token: Option<&str>) -> StatusCode {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        let request = builder.body(Body::empty()).unwrap();
        app.clone().oneshot(request).await.unwrap().status()
    }

    /// Issues `GET /rooms` with whatever query params are given and decodes the `RoomList` body.
    async fn get_rooms(app: &Router, token: &str, q: Option<&str>, limit: Option<usize>, cursor: Option<&str>) -> RoomList {
        let mut params = Vec::new();
        if let Some(q) = q {
            params.push(format!("q={q}"));
        }
        if let Some(limit) = limit {
            params.push(format!("limit={limit}"));
        }
        if let Some(cursor) = cursor {
            params.push(format!("cursor={cursor}"));
        }
        let uri = if params.is_empty() { "/rooms".to_string() } else { format!("/rooms?{}", params.join("&")) };
        let request =
            Request::builder().method("GET").uri(uri).header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[tokio::test]
    async fn health_needs_no_token() {
        let (app, _store) = app();
        assert_eq!(request(&app, "GET", "/health", None).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn unknown_path_is_not_found_even_with_no_token() {
        let (app, _store) = app();
        assert_eq!(request(&app, "GET", "/nonsense", None).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn auth_me_requires_a_token() {
        let (app, _store) = app();
        assert_eq!(request(&app, "GET", "/auth/me", None).await, StatusCode::UNAUTHORIZED);
        assert_eq!(request(&app, "GET", "/auth/me", Some("unknown")).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_me_returns_the_callers_own_code() {
        let (app, store) = app();
        store.seed("member", false);

        let request = Request::builder()
            .method("GET")
            .uri("/auth/me")
            .header("authorization", "Bearer member")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let code: AccessCode = serde_json::from_slice(&body).unwrap();
        assert_eq!(code.access_key.to_string(), "member");
        assert!(!code.is_admin);
    }

    #[tokio::test]
    async fn rooms_requires_an_admin_code() {
        let (app, store) = app();
        store.seed("member", false);
        store.seed("root", true);
        assert_eq!(request(&app, "GET", "/rooms", Some("member")).await, StatusCode::FORBIDDEN);
        assert_eq!(request(&app, "GET", "/rooms", Some("root")).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn rooms_requires_a_token() {
        let (app, _store) = app();
        assert_eq!(request(&app, "GET", "/rooms", None).await, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn rooms_search_is_a_case_insensitive_substring_match_and_blank_means_everything() {
        let (app, access_codes, rooms, _hub) = full_app();
        access_codes.seed("root", true);
        rooms.seed(sample_room("goblin-camp"));
        rooms.seed(sample_room("dragons-lair"));

        let matched = get_rooms(&app, "root", Some("GOB"), None, None).await;
        assert_eq!(matched.rooms.len(), 1);
        assert_eq!(matched.rooms[0].display_name.to_string(), "goblin-camp");

        let everything = get_rooms(&app, "root", None, None, None).await;
        assert_eq!(everything.rooms.len(), 2);
    }

    #[tokio::test]
    async fn rooms_pages_through_every_matching_room_with_no_gaps_or_duplicates() {
        let (app, access_codes, rooms, _hub) = full_app();
        access_codes.seed("root", true);
        for i in 0..5 {
            rooms.seed(sample_room(&format!("room-{i}")));
        }

        let mut seen = std::collections::HashSet::new();
        let mut cursor: Option<String> = None;
        loop {
            let page = get_rooms(&app, "root", None, Some(2), cursor.as_deref()).await;
            assert!(page.rooms.len() <= 2);
            for room in &page.rooms {
                assert!(seen.insert(room.display_name.to_string()), "room reported twice across pages");
            }
            match page.next_cursor {
                Some(next) => cursor = Some(next),
                None => break,
            }
        }
        assert_eq!(seen.len(), 5);
    }

    #[tokio::test]
    async fn deleting_a_room_requires_admin_and_removes_it() {
        let (app, access_codes, rooms, _hub) = full_app();
        access_codes.seed("member", false);
        access_codes.seed("root", true);
        rooms.seed(sample_room("goblin-camp"));

        assert_eq!(request(&app, "DELETE", "/rooms/goblin-camp", Some("member")).await, StatusCode::FORBIDDEN);
        assert_eq!(request(&app, "DELETE", "/rooms/goblin-camp", Some("root")).await, StatusCode::NO_CONTENT);
        // Gone: a second delete finds nothing to remove.
        assert_eq!(request(&app, "DELETE", "/rooms/goblin-camp", Some("root")).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn deleting_an_unknown_room_is_not_found() {
        let (app, access_codes, _rooms, _hub) = full_app();
        access_codes.seed("root", true);
        assert_eq!(request(&app, "DELETE", "/rooms/nowhere", Some("root")).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn closing_a_room_notifies_every_member_it_was_closed() {
        let (app, access_codes, rooms, hub) = full_app();
        access_codes.seed("root", true);

        let member_id = ConnectionId("member-conn".to_string());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        hub.register(member_id.clone(), tx);

        let mut room = sample_room("goblin-camp");
        room.members.push(RoomMember { connection_id: member_id, name: "Guest".to_string(), can_write: true, is_host: true });
        rooms.seed(room);

        assert_eq!(request(&app, "DELETE", "/rooms/goblin-camp", Some("root")).await, StatusCode::NO_CONTENT);

        let sent = rx.try_recv().expect("the member should have been notified that its room closed");
        let axum::extract::ws::Message::Text(text) = sent else { panic!("expected a text frame") };
        let envelope: shared::protocol::ServerEnvelope = serde_json::from_str(&text).unwrap();
        assert_eq!(envelope.reply_to, None);
        assert_eq!(envelope.message, ServerMessage::Left { reason: LeaveReason::RoomClosed });
    }

    #[tokio::test]
    async fn access_codes_requires_admin() {
        let (app, store) = app();
        store.seed("member", false);
        store.seed("root", true);

        assert_eq!(request(&app, "GET", "/access-codes", Some("member")).await, StatusCode::FORBIDDEN);
        assert_eq!(request(&app, "GET", "/access-codes", Some("root")).await, StatusCode::OK);
    }

    #[tokio::test]
    async fn admin_can_create_read_update_and_delete_a_code() {
        let (app, store) = app();
        store.seed("root", true);

        let create = Request::builder()
            .method("POST")
            .uri("/access-codes")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"is_admin":false}"#))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: AccessCode = serde_json::from_slice(&body).unwrap();
        assert!(!created.is_admin);

        let path = format!("/access-codes/{}", created.access_key.to_string());

        assert_eq!(request(&app, "GET", &path, Some("root")).await, StatusCode::OK);

        let update = Request::builder()
            .method("PUT")
            .uri(&path)
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"is_admin":true}"#))
            .unwrap();
        assert_eq!(app.clone().oneshot(update).await.unwrap().status(), StatusCode::OK);

        assert_eq!(request(&app, "DELETE", &path, Some("root")).await, StatusCode::NO_CONTENT);
        assert_eq!(request(&app, "GET", &path, Some("root")).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn admin_can_create_a_code_with_a_chosen_key() {
        let (app, store) = app();
        store.seed("root", true);

        let create = Request::builder()
            .method("POST")
            .uri("/access-codes")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"access_key":"player-one","is_admin":false}"#))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: AccessCode = serde_json::from_slice(&body).unwrap();
        assert_eq!(created.access_key.to_string(), "player-one");
    }

    #[tokio::test]
    async fn creating_a_code_missing_a_required_field_is_a_bad_request() {
        let (app, store) = app();
        store.seed("root", true);

        // No `is_admin` at all -- valid JSON, wrong shape for `CreateAccessCode`, which the
        // schema (not just serde's own field-presence check) catches before this ever reaches
        // the handler.
        let create = Request::builder()
            .method("POST")
            .uri("/access-codes")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"access_key":"player-one"}"#))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn creating_a_code_with_an_empty_access_key_is_a_bad_request() {
        let (app, store) = app();
        store.seed("root", true);

        let create = Request::builder()
            .method("POST")
            .uri("/access-codes")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"access_key":"","is_admin":false}"#))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn creating_a_code_with_malformed_json_is_a_bad_request() {
        let (app, store) = app();
        store.seed("root", true);

        let create = Request::builder()
            .method("POST")
            .uri("/access-codes")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from("{not json"))
            .unwrap();
        let response = app.clone().oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn creating_a_code_with_a_colliding_key_is_a_conflict() {
        let (app, store) = app();
        store.seed("root", true);
        store.seed("player-one", false);

        let create = Request::builder()
            .method("POST")
            .uri("/access-codes")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"access_key":"player-one","is_admin":false}"#))
            .unwrap();
        assert_eq!(app.oneshot(create).await.unwrap().status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn a_blank_key_still_generates_one() {
        let (app, store) = app();
        store.seed("root", true);

        let create = Request::builder()
            .method("POST")
            .uri("/access-codes")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"access_key":"  ","is_admin":false}"#))
            .unwrap();
        let response = app.oneshot(create).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let created: AccessCode = serde_json::from_slice(&body).unwrap();
        assert!(!created.access_key.trim().is_empty());
    }

    #[tokio::test]
    async fn deleting_an_unknown_code_is_not_found() {
        let (app, store) = app();
        store.seed("root", true);
        assert_eq!(request(&app, "DELETE", "/access-codes/unknown", Some("root")).await, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn an_admin_cannot_demote_or_delete_themselves() {
        let (app, store) = app();
        store.seed("root", true);

        let demote = Request::builder()
            .method("PUT")
            .uri("/access-codes/root")
            .header("authorization", "Bearer root")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"is_admin":false}"#))
            .unwrap();
        assert_eq!(app.clone().oneshot(demote).await.unwrap().status(), StatusCode::FORBIDDEN);

        assert_eq!(request(&app, "DELETE", "/access-codes/root", Some("root")).await, StatusCode::FORBIDDEN);
    }
}
