use crate::access_codes::{AccessCode, AccessCodeStore};
use crate::auth::{require_access_code, require_admin, Caller};
use crate::error::ApiError;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{middleware, Json, Router};
use shared::access::{AccessCodeList, CreateAccessCode, UpdateAccessCode};

#[derive(Clone)]
pub struct AppState<S> {
    pub access_codes: S,
}

pub fn router<S: AccessCodeStore>(state: AppState<S>) -> Router {
    let admin = Router::new()
        .route("/access-codes", get(list_access_codes::<S>).post(create_access_code::<S>))
        .route(
            "/access-codes/{access_key}",
            get(read_access_code::<S>).put(update_access_code::<S>).delete(delete_access_code::<S>),
        )
        .route_layer(middleware::from_fn(require_admin));

    // `/auth/me` and `/access-codes/{access_key}` are siblings in one matchit tree, which always
    // prefers a static segment over a parameter -- but every route naming the key must spell it
    // `{access_key}`, since matchit treats two different parameter names at the same position as a
    // routing conflict.
    let authenticated = Router::new()
        .route("/auth/me", get(my_access_code))
        .merge(admin)
        .route_layer(middleware::from_fn_with_state(state.clone(), require_access_code::<S>));

    // `route_layer`, not `layer`: it skips the fallback, so an unauthenticated request to an
    // unknown path stays a 404 instead of becoming a 401. `/health` is added to the outer router
    // afterward, so it is never wrapped by `require_access_code` -- the pod's liveness and
    // readiness probes need to reach it with no token.
    Router::new().route("/health", get(health)).merge(authenticated).fallback(not_found).with_state(state)
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

async fn list_access_codes<S: AccessCodeStore>(State(state): State<AppState<S>>) -> Result<Json<AccessCodeList>, ApiError> {
    let codes = state.access_codes.list().await?;
    Ok(Json(AccessCodeList { codes }))
}

async fn create_access_code<S: AccessCodeStore>(
    State(state): State<AppState<S>>,
    Json(body): Json<CreateAccessCode>,
) -> Result<(StatusCode, Json<AccessCode>), ApiError> {
    let access_key = body.access_key.as_deref().map(str::trim).filter(|key| !key.is_empty());
    let code = state.access_codes.create(access_key, body.is_admin).await?;
    Ok((StatusCode::CREATED, Json(code)))
}

async fn read_access_code<S: AccessCodeStore>(
    State(state): State<AppState<S>>,
    Path(access_key): Path<String>,
) -> Result<Json<AccessCode>, ApiError> {
    let code = state.access_codes.get(&access_key).await?.ok_or(ApiError::NotFound)?;
    Ok(Json(code))
}

async fn update_access_code<S: AccessCodeStore>(
    Caller(caller): Caller,
    State(state): State<AppState<S>>,
    Path(access_key): Path<String>,
    Json(body): Json<UpdateAccessCode>,
) -> Result<Json<AccessCode>, ApiError> {
    // An admin can't demote or delete themselves through this API -- without this, the only way
    // back from locking out the last admin is the AWS console or CLI.
    if caller.access_key == access_key {
        return Err(ApiError::SelfModification);
    }
    let code = state.access_codes.update(&access_key, body.is_admin).await?;
    Ok(Json(code))
}

async fn delete_access_code<S: AccessCodeStore>(
    Caller(caller): Caller,
    State(state): State<AppState<S>>,
    Path(access_key): Path<String>,
) -> Result<StatusCode, ApiError> {
    if caller.access_key == access_key {
        return Err(ApiError::SelfModification);
    }
    state.access_codes.delete(&access_key).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::access_codes::MemoryAccessCodeStore;
    use axum::body::{to_bytes, Body};
    use axum::http::Request;
    use tower::ServiceExt;

    fn app() -> (Router, MemoryAccessCodeStore) {
        let store = MemoryAccessCodeStore::default();
        (router(AppState { access_codes: store.clone() }), store)
    }

    async fn request(app: &Router, method: &str, path: &str, token: Option<&str>) -> StatusCode {
        let mut builder = Request::builder().method(method).uri(path);
        if let Some(token) = token {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        let request = builder.body(Body::empty()).unwrap();
        app.clone().oneshot(request).await.unwrap().status()
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
        assert_eq!(code.access_key, "member");
        assert!(!code.is_admin);
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

        let path = format!("/access-codes/{}", created.access_key);

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
        assert_eq!(created.access_key, "player-one");
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
