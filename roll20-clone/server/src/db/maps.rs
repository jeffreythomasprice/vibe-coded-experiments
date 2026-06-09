//! Persistence for [`Map`]s.
//!
//! A map is stored as scalar metadata columns plus a single JSON `content` blob
//! holding `{groups, shapes}`. Reads/writes are always whole-map, so the blob
//! avoids a relational decomposition of the recursive boolean-operator tree.

use anyhow::{anyhow, Context};
use serde::{Deserialize, Serialize};
use shared::{Group, Map, MapSummary, Shape};
use turso::{Connection, Value};

/// The JSON-blob portion of a map (everything not promoted to a column).
#[derive(Serialize, Deserialize)]
struct Content {
    groups: Vec<Group>,
    shapes: Vec<Shape>,
}

/// List every map as a lightweight summary (no shapes/groups parsed).
pub async fn list(conn: &Connection) -> anyhow::Result<Vec<MapSummary>> {
    let mut rows = conn
        .query(
            "SELECT id, name, width, height, grid_size, grid_unit FROM maps ORDER BY name",
            (),
        )
        .await
        .context("listing maps")?;

    let mut out = Vec::new();
    while let Some(row) = rows.next().await? {
        out.push(MapSummary {
            id: text(&row, 0)?,
            name: text(&row, 1)?,
            width: integer(&row, 2)? as u32,
            height: integer(&row, 3)? as u32,
            grid_size: real(&row, 4)?,
            grid_unit: text(&row, 5)?,
        });
    }
    Ok(out)
}

/// Fetch a single map by id, or `None` if it does not exist.
pub async fn get(conn: &Connection, id: &str) -> anyhow::Result<Option<Map>> {
    let mut rows = conn
        .query(
            "SELECT id, name, width, height, grid_size, grid_unit, \
             background_color, grid_color, content FROM maps WHERE id = ?1",
            [id],
        )
        .await
        .context("fetching map")?;

    let Some(row) = rows.next().await? else {
        return Ok(None);
    };

    let content: Content =
        serde_json::from_str(&text(&row, 8)?).context("parsing map content JSON")?;

    Ok(Some(Map {
        id: text(&row, 0)?,
        name: text(&row, 1)?,
        width: integer(&row, 2)? as u32,
        height: integer(&row, 3)? as u32,
        grid_size: real(&row, 4)?,
        grid_unit: text(&row, 5)?,
        background_color: text(&row, 6)?,
        grid_color: text(&row, 7)?,
        groups: content.groups,
        shapes: content.shapes,
    }))
}

/// Insert a brand-new map.
pub async fn insert(conn: &Connection, map: &Map) -> anyhow::Result<()> {
    let content = content_json(map)?;
    conn.execute(
        "INSERT INTO maps \
         (id, name, width, height, grid_size, grid_unit, background_color, grid_color, content, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, strftime('%s', 'now'))",
        (
            map.id.clone(),
            map.name.clone(),
            map.width,
            map.height,
            map.grid_size,
            map.grid_unit.clone(),
            map.background_color.clone(),
            map.grid_color.clone(),
            content,
        ),
    )
    .await
    .context("inserting map")?;
    Ok(())
}

/// Overwrite an existing map's metadata and content.
pub async fn update(conn: &Connection, map: &Map) -> anyhow::Result<()> {
    let content = content_json(map)?;
    conn.execute(
        "UPDATE maps SET \
         name = ?2, width = ?3, height = ?4, grid_size = ?5, grid_unit = ?6, \
         background_color = ?7, grid_color = ?8, content = ?9, updated_at = strftime('%s', 'now') \
         WHERE id = ?1",
        (
            map.id.clone(),
            map.name.clone(),
            map.width,
            map.height,
            map.grid_size,
            map.grid_unit.clone(),
            map.background_color.clone(),
            map.grid_color.clone(),
            content,
        ),
    )
    .await
    .context("updating map")?;
    Ok(())
}

/// Delete a map by id. Returns whether a row was removed.
pub async fn delete(conn: &Connection, id: &str) -> anyhow::Result<bool> {
    let affected = conn
        .execute("DELETE FROM maps WHERE id = ?1", [id])
        .await
        .context("deleting map")?;
    Ok(affected > 0)
}

fn content_json(map: &Map) -> anyhow::Result<String> {
    serde_json::to_string(&Content {
        groups: map.groups.clone(),
        shapes: map.shapes.clone(),
    })
    .context("serializing map content")
}

// --- typed column accessors -------------------------------------------------

fn text(row: &turso::Row, idx: usize) -> anyhow::Result<String> {
    match row.get_value(idx)? {
        Value::Text(s) => Ok(s),
        other => Err(anyhow!("column {idx}: expected TEXT, got {other:?}")),
    }
}

fn integer(row: &turso::Row, idx: usize) -> anyhow::Result<i64> {
    match row.get_value(idx)? {
        Value::Integer(n) => Ok(n),
        other => Err(anyhow!("column {idx}: expected INTEGER, got {other:?}")),
    }
}

fn real(row: &turso::Row, idx: usize) -> anyhow::Result<f64> {
    match row.get_value(idx)? {
        Value::Real(f) => Ok(f),
        // SQLite may store a whole-number REAL as INTEGER.
        Value::Integer(n) => Ok(n as f64),
        other => Err(anyhow!("column {idx}: expected REAL, got {other:?}")),
    }
}
