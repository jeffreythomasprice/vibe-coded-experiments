use std::collections::BTreeSet;

use editor_core::{Document, EntityId, ObjectId, Value};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::component_defs::Catalog;

const MAX_DEPTH: usize = 32;

/// A string in an entity's properties that its class schema marks as an entity id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// JSON pointer inside the properties object.
    pub pointer: String,
    pub value: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LinkStatus {
    /// The target entity exists.
    Ok,
    /// A valid entity id, but no such entity.
    Missing,
    /// An id of a brush, mesh, group or other non-entity.
    NotEntity,
    /// Not an object id at all.
    Malformed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntityLink {
    pub from: EntityId,
    /// The component holding the link.
    pub component: String,
    /// JSON pointer inside that component's data.
    pub pointer: String,
    /// The stored value.
    pub value: String,
    pub status: LinkStatus,
    /// The target, when `status` is `ok`.
    pub to: Option<EntityId>,
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn is_link_node(schema: &Value) -> bool {
    schema
        .get("editor")
        .and_then(|e| e.get("linkType"))
        .and_then(Value::as_str)
        == Some("entityID")
}

fn walk(root: &Value, schema: &Value, value: &Value, ptr: &str, depth: usize, out: &mut Vec<Link>) {
    if depth > MAX_DEPTH || !schema.is_object() {
        return;
    }
    if let Some(target) = schema
        .get("$ref")
        .and_then(Value::as_str)
        .and_then(|r| r.strip_prefix('#'))
        && let Some(resolved) = root.pointer(target)
    {
        walk(root, resolved, value, ptr, depth + 1, out);
    }
    if is_link_node(schema)
        && let Some(s) = value.as_str()
    {
        out.push(Link {
            pointer: ptr.to_string(),
            value: s.to_string(),
        });
    }
    for key in ["allOf", "anyOf", "oneOf"] {
        if let Some(branches) = schema.get(key).and_then(Value::as_array) {
            for branch in branches {
                walk(root, branch, value, ptr, depth + 1, out);
            }
        }
    }
    match value {
        Value::Object(map) => {
            let props = schema.get("properties").and_then(Value::as_object);
            for (k, v) in map {
                let child = format!("{ptr}/{}", escape(k));
                match props.and_then(|p| p.get(k)) {
                    Some(sub) => walk(root, sub, v, &child, depth + 1, out),
                    None => {
                        if let Some(extra) = schema.get("additionalProperties") {
                            walk(root, extra, v, &child, depth + 1, out);
                        }
                    }
                }
            }
        }
        Value::Array(items) => {
            let prefix = schema.get("prefixItems").and_then(Value::as_array);
            for (i, v) in items.iter().enumerate() {
                let child = format!("{ptr}/{i}");
                if let Some(sub) = prefix.and_then(|p| p.get(i)) {
                    walk(root, sub, v, &child, depth + 1, out);
                } else if let Some(sub) = schema.get("items") {
                    walk(root, sub, v, &child, depth + 1, out);
                }
            }
        }
        _ => {}
    }
}

/// Every link in `value`, in document order of discovery, without duplicates. Union of `anyOf`/`oneOf` branches is used, so a pointer counts if any branch marks it. `patternProperties` is not followed.
pub fn links_of(schema: &Value, value: &Value) -> Vec<Link> {
    let mut found = Vec::new();
    walk(schema, schema, value, "", 0, &mut found);
    let mut seen = BTreeSet::new();
    found.retain(|l| seen.insert(l.pointer.clone()));
    found
}

pub fn status_of(doc: &Document, value: &str) -> (LinkStatus, Option<EntityId>) {
    match value.parse::<ObjectId>() {
        Ok(ObjectId::Entity(e)) if doc.entities.contains_key(&e) => (LinkStatus::Ok, Some(e)),
        Ok(ObjectId::Entity(_)) => (LinkStatus::Missing, None),
        Ok(_) => (LinkStatus::NotEntity, None),
        Err(_) => (LinkStatus::Malformed, None),
    }
}

pub fn doc_links(doc: &Document, catalog: &Catalog) -> Vec<EntityLink> {
    let mut out = Vec::new();
    for entity in doc.entities.values() {
        for (name, data) in &entity.components {
            for link in catalog.links(name, data) {
                let (status, to) = status_of(doc, &link.value);
                out.push(EntityLink {
                    from: entity.id,
                    component: name.clone(),
                    pointer: link.pointer,
                    value: link.value,
                    status,
                    to,
                });
            }
        }
    }
    out
}
