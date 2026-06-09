//! Typed wrappers around the server's JSON HTTP API.

use gloo_net::http::Request;
use shared::{
    CreateMapRequest, CreateShapeRequest, Map, MapSummary, UpdateGroupRequest, UpdateShapeRequest,
};

/// HTTP base URL of the server, baked in at build time from `client/.env`.
pub const SERVER_HTTP_URL: &str = env!("SERVER_HTTP_URL");

fn url(path: &str) -> String {
    format!("{SERVER_HTTP_URL}{path}")
}

async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T, String> {
    Request::get(&url(path))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn send_json<B: serde::Serialize, T: serde::de::DeserializeOwned>(
    method: &str,
    path: &str,
    body: &B,
) -> Result<T, String> {
    let builder = match method {
        "POST" => Request::post(&url(path)),
        "PUT" => Request::put(&url(path)),
        other => return Err(format!("unsupported method {other}")),
    };
    builder
        .json(body)
        .map_err(|e| e.to_string())?
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_maps() -> Result<Vec<MapSummary>, String> {
    get_json("/api/maps").await
}

pub async fn get_map(id: &str) -> Result<Map, String> {
    get_json(&format!("/api/maps/{id}")).await
}

pub async fn create_map(req: &CreateMapRequest) -> Result<Map, String> {
    send_json("POST", "/api/maps", req).await
}

pub async fn add_shape(map_id: &str, req: &CreateShapeRequest) -> Result<Map, String> {
    send_json("POST", &format!("/api/maps/{map_id}/shapes"), req).await
}

pub async fn update_shape(
    map_id: &str,
    shape_id: &str,
    req: &UpdateShapeRequest,
) -> Result<Map, String> {
    send_json("PUT", &format!("/api/maps/{map_id}/shapes/{shape_id}"), req).await
}

pub async fn update_group(
    map_id: &str,
    group_id: &str,
    req: &UpdateGroupRequest,
) -> Result<Map, String> {
    send_json("PUT", &format!("/api/maps/{map_id}/groups/{group_id}"), req).await
}

/// A sensible default map for the "New map" button.
pub fn default_create_request(name: &str) -> CreateMapRequest {
    CreateMapRequest {
        name: name.to_string(),
        width: 20,
        height: 15,
        grid_size: 5.0,
        grid_unit: "ft".to_string(),
        background_color: "#1e1e28".to_string(),
        grid_color: "#3a3a4a".to_string(),
    }
}
