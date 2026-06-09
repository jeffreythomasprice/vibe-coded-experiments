//! HTTP handlers for map CRUD and the shapes/groups within a map.
//!
//! Every mutation follows the same flow: load the map, mutate it in Rust,
//! persist the whole map, broadcast a [`ServerMessage::MapUpdated`] to followers,
//! and return the updated map.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use shared::{
    CreateGroupRequest, CreateMapRequest, CreateShapeRequest, Group, Map, MapSummary, ServerMessage,
    Shape, UpdateGroupRequest, UpdateMapRequest, UpdateShapeRequest,
};
use turso::Connection;
use uuid::Uuid;

use crate::db;
use crate::error::ApiError;
use crate::state::AppState;

/// Obtain a database connection from shared state.
fn conn(state: &AppState) -> Result<Connection, ApiError> {
    state
        .db()
        .connect()
        .map_err(|e| ApiError::Internal(e.into()))
}

/// Persist `map`, broadcast it to followers, and return it as JSON.
async fn save_and_broadcast(
    state: &AppState,
    conn: &Connection,
    map: Map,
) -> Result<Json<Map>, ApiError> {
    db::maps::update(conn, &map).await?;
    state.broadcast(ServerMessage::MapUpdated { map: map.clone() });
    Ok(Json(map))
}

// --- maps -------------------------------------------------------------------

pub async fn list_maps(State(state): State<AppState>) -> Result<Json<Vec<MapSummary>>, ApiError> {
    let conn = conn(&state)?;
    Ok(Json(db::maps::list(&conn).await?))
}

pub async fn get_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    db::maps::get(&conn, &id).await?.map(Json).ok_or(ApiError::NotFound)
}

pub async fn create_map(
    State(state): State<AppState>,
    Json(req): Json<CreateMapRequest>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let map = Map {
        id: Uuid::new_v4().to_string(),
        name: req.name,
        width: req.width,
        height: req.height,
        grid_size: req.grid_size,
        grid_unit: req.grid_unit,
        background_color: req.background_color,
        grid_color: req.grid_color,
        groups: Vec::new(),
        shapes: Vec::new(),
    };
    db::maps::insert(&conn, &map).await?;
    state.broadcast(ServerMessage::MapUpdated { map: map.clone() });
    Ok(Json(map))
}

pub async fn update_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateMapRequest>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let mut map = db::maps::get(&conn, &id).await?.ok_or(ApiError::NotFound)?;
    if let Some(v) = req.name {
        map.name = v;
    }
    if let Some(v) = req.background_color {
        map.background_color = v;
    }
    if let Some(v) = req.grid_color {
        map.grid_color = v;
    }
    if let Some(v) = req.grid_size {
        map.grid_size = v;
    }
    if let Some(v) = req.grid_unit {
        map.grid_unit = v;
    }
    save_and_broadcast(&state, &conn, map).await
}

pub async fn delete_map(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    let conn = conn(&state)?;
    if db::maps::delete(&conn, &id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::NotFound)
    }
}

// --- shapes -----------------------------------------------------------------

pub async fn add_shape(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateShapeRequest>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let mut map = db::maps::get(&conn, &id).await?.ok_or(ApiError::NotFound)?;
    map.shapes.push(Shape {
        id: Uuid::new_v4().to_string(),
        geometry: req.geometry,
        style: req.style,
    });
    save_and_broadcast(&state, &conn, map).await
}

pub async fn update_shape(
    State(state): State<AppState>,
    Path((id, shape_id)): Path<(String, String)>,
    Json(req): Json<UpdateShapeRequest>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let mut map = db::maps::get(&conn, &id).await?.ok_or(ApiError::NotFound)?;
    let shape = map
        .shapes
        .iter_mut()
        .find(|s| s.id == shape_id)
        .ok_or(ApiError::NotFound)?;
    if let Some(g) = req.geometry {
        shape.geometry = g;
    }
    if let Some(s) = req.style {
        shape.style = s;
    }
    save_and_broadcast(&state, &conn, map).await
}

pub async fn delete_shape(
    State(state): State<AppState>,
    Path((id, shape_id)): Path<(String, String)>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let mut map = db::maps::get(&conn, &id).await?.ok_or(ApiError::NotFound)?;
    let before = map.shapes.len();
    map.shapes.retain(|s| s.id != shape_id);
    if map.shapes.len() == before {
        return Err(ApiError::NotFound);
    }
    save_and_broadcast(&state, &conn, map).await
}

// --- groups -----------------------------------------------------------------

pub async fn add_group(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let mut map = db::maps::get(&conn, &id).await?.ok_or(ApiError::NotFound)?;
    map.groups.push(Group {
        id: Uuid::new_v4().to_string(),
        style: req.style,
        root: req.root,
    });
    save_and_broadcast(&state, &conn, map).await
}

pub async fn update_group(
    State(state): State<AppState>,
    Path((id, group_id)): Path<(String, String)>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let mut map = db::maps::get(&conn, &id).await?.ok_or(ApiError::NotFound)?;
    let group = map
        .groups
        .iter_mut()
        .find(|g| g.id == group_id)
        .ok_or(ApiError::NotFound)?;
    if let Some(s) = req.style {
        group.style = s;
    }
    if let Some(r) = req.root {
        group.root = r;
    }
    save_and_broadcast(&state, &conn, map).await
}

pub async fn delete_group(
    State(state): State<AppState>,
    Path((id, group_id)): Path<(String, String)>,
) -> Result<Json<Map>, ApiError> {
    let conn = conn(&state)?;
    let mut map = db::maps::get(&conn, &id).await?.ok_or(ApiError::NotFound)?;
    let before = map.groups.len();
    map.groups.retain(|g| g.id != group_id);
    if map.groups.len() == before {
        return Err(ApiError::NotFound);
    }
    save_and_broadcast(&state, &conn, map).await
}
