use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chess_shared::{CreateUserRequest, ListUsersResponse, UpdateUserRequest, UserSummary};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::extractors::ValidatedJson;
use crate::AppState;

#[derive(Deserialize)]
pub struct ListUsersParams {
    pub page_size: Option<u32>,
    pub page_token: Option<String>,
    pub search: Option<String>,
}

#[derive(Deserialize, serde::Serialize)]
struct Cursor {
    created_at: DateTime<Utc>,
    id: Uuid,
}

fn forbidden(msg: &str) -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::FORBIDDEN,
        Json(serde_json::json!({"error": msg})),
    )
}

fn internal_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "internal_error"})),
    )
}

fn not_found() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error": "not_found"})),
    )
}

pub async fn create_user(
    auth_user: AuthUser,
    State(state): State<AppState>,
    ValidatedJson(payload): ValidatedJson<CreateUserRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !auth_user.is_admin {
        return Err(forbidden("admin_required"));
    }

    let row = sqlx::query_as::<_, (Uuid, String, bool, DateTime<Utc>)>(
        "INSERT INTO users (username, password, is_admin) VALUES ($1, $2, $3) RETURNING id, username, is_admin, created_at",
    )
    .bind(payload.username.as_str())
    .bind(payload.password.as_str())
    .bind(payload.is_admin)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.code().as_deref() == Some("23505") {
                return (
                    StatusCode::CONFLICT,
                    Json(serde_json::json!({"error": "username_taken"})),
                );
            }
        }
        internal_error()
    })?;

    Ok((
        StatusCode::CREATED,
        Json(UserSummary {
            id: row.0,
            username: row.1,
            is_admin: row.2,
            created_at: row.3,
        }),
    ))
}

pub async fn list_users(
    _auth_user: AuthUser,
    State(state): State<AppState>,
    Query(params): Query<ListUsersParams>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let page_size = params.page_size.unwrap_or(20).min(100) as i64;
    let fetch_limit = page_size + 1;

    let cursor = params
        .page_token
        .as_deref()
        .map(|token| {
            let bytes = URL_SAFE_NO_PAD
                .decode(token)
                .map_err(|_| forbidden("invalid_page_token"))?;
            serde_json::from_slice::<Cursor>(&bytes)
                .map_err(|_| forbidden("invalid_page_token"))
        })
        .transpose()?;

    let search_pattern = params.search.as_deref().map(|s| format!("{s}%"));

    let rows: Vec<(Uuid, String, bool, DateTime<Utc>)> = match (&cursor, &search_pattern) {
        (Some(cursor), Some(pattern)) => {
            sqlx::query_as(
                "SELECT id, username, is_admin, created_at FROM users \
                 WHERE (created_at, id) > ($1, $2) AND username ILIKE $3 \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $4",
            )
            .bind(cursor.created_at)
            .bind(cursor.id)
            .bind(pattern)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        }
        (Some(cursor), None) => {
            sqlx::query_as(
                "SELECT id, username, is_admin, created_at FROM users \
                 WHERE (created_at, id) > ($1, $2) \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $3",
            )
            .bind(cursor.created_at)
            .bind(cursor.id)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        }
        (None, Some(pattern)) => {
            sqlx::query_as(
                "SELECT id, username, is_admin, created_at FROM users \
                 WHERE username ILIKE $1 \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $2",
            )
            .bind(pattern)
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        }
        (None, None) => {
            sqlx::query_as(
                "SELECT id, username, is_admin, created_at FROM users \
                 ORDER BY created_at ASC, id ASC \
                 LIMIT $1",
            )
            .bind(fetch_limit)
            .fetch_all(&state.db)
            .await
            .map_err(|_| internal_error())?
        }
    };

    let has_next = rows.len() as i64 > page_size;
    let users: Vec<UserSummary> = rows
        .into_iter()
        .take(page_size as usize)
        .map(|(id, username, is_admin, created_at)| UserSummary {
            id,
            username,
            is_admin,
            created_at,
        })
        .collect();

    let next_page_token = if has_next {
        users.last().map(|u| {
            let cursor = Cursor {
                created_at: u.created_at,
                id: u.id,
            };
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&cursor).unwrap())
        })
    } else {
        None
    };

    Ok::<_, (StatusCode, Json<serde_json::Value>)>((
        StatusCode::OK,
        Json(ListUsersResponse {
            users,
            next_page_token,
        }),
    ))
}

pub async fn get_user(
    _auth_user: AuthUser,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<(StatusCode, Json<UserSummary>), (StatusCode, Json<serde_json::Value>)> {
    let row = sqlx::query_as::<_, (Uuid, String, bool, DateTime<Utc>)>(
        "SELECT id, username, is_admin, created_at FROM users WHERE username = $1",
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| internal_error())?;

    let (id, username, is_admin, created_at) = row.ok_or_else(not_found)?;

    Ok((
        StatusCode::OK,
        Json(UserSummary {
            id,
            username,
            is_admin,
            created_at,
        }),
    ))
}

pub async fn update_user(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(username): Path<String>,
    ValidatedJson(payload): ValidatedJson<UpdateUserRequest>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    let is_self = auth_user.username == username;

    if !auth_user.is_admin && !is_self {
        return Err(forbidden("admin_required"));
    }

    // Non-admins can only change their own password
    if !auth_user.is_admin && payload.is_admin.is_some() {
        return Err(forbidden("admin_required"));
    }

    if is_self && payload.is_admin == Some(false) {
        return Err(forbidden("cannot_demote_self"));
    }

    // Build dynamic UPDATE query
    let mut set_clauses = Vec::new();
    let mut param_index = 1u32;

    if payload.is_admin.is_some() {
        set_clauses.push(format!("is_admin = ${param_index}"));
        param_index += 1;
    }
    if payload.password.is_some() {
        set_clauses.push(format!("password = ${param_index}"));
        param_index += 1;
    }

    if set_clauses.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "no_fields_to_update"})),
        ));
    }

    let sql = format!(
        "UPDATE users SET {} WHERE username = ${param_index} RETURNING id, username, is_admin, created_at",
        set_clauses.join(", ")
    );

    let mut query = sqlx::query_as::<_, (Uuid, String, bool, DateTime<Utc>)>(&sql);
    if let Some(is_admin) = payload.is_admin {
        query = query.bind(is_admin);
    }
    if let Some(ref password) = payload.password {
        query = query.bind(password.as_str());
    }
    query = query.bind(&username);

    let row = query
        .fetch_optional(&state.db)
        .await
        .map_err(|_| internal_error())?;

    let (id, username, is_admin, created_at) = row.ok_or_else(not_found)?;

    Ok((
        StatusCode::OK,
        Json(UserSummary {
            id,
            username,
            is_admin,
            created_at,
        }),
    ))
}

pub async fn delete_user(
    auth_user: AuthUser,
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse, impl IntoResponse> {
    if !auth_user.is_admin {
        return Err(forbidden("admin_required"));
    }

    if auth_user.username == username {
        return Err(forbidden("cannot_delete_self"));
    }

    let result = sqlx::query("DELETE FROM users WHERE username = $1")
        .bind(&username)
        .execute(&state.db)
        .await
        .map_err(|_| internal_error())?;

    if result.rows_affected() == 0 {
        return Err(not_found());
    }

    Ok(StatusCode::NO_CONTENT)
}
