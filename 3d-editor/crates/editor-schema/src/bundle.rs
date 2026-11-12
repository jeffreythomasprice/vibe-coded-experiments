use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::BundleError;

pub const DRAFT: &str = "https://json-schema.org/draft/2020-12/schema";

/// Merges the `$defs` of every file into one namespace, so a type is named the same everywhere.
/// A `$ref` into another file (`common.schema.json#/$defs/X`) becomes a local `#/$defs/X`.
pub fn bundle(files: &[(&str, &str)]) -> Result<Map<String, Value>, BundleError> {
    let mut merged: BTreeMap<String, (String, Value)> = BTreeMap::new();
    for (file, text) in files {
        let mut root: Value = serde_json::from_str(text).map_err(|e| BundleError::Syntax {
            file: (*file).to_string(),
            message: e.to_string(),
        })?;
        let Some(defs) = root.get_mut("$defs").and_then(Value::as_object_mut) else {
            continue;
        };
        for (name, mut def) in std::mem::take(defs) {
            localize_refs(&mut def);
            if let Some((first, _)) = merged.insert(name.clone(), ((*file).to_string(), def)) {
                return Err(BundleError::DuplicateDef {
                    name,
                    first,
                    second: (*file).to_string(),
                });
            }
        }
    }
    let mut out = Map::new();
    out.insert("$schema".into(), Value::String(DRAFT.into()));
    out.insert(
        "$defs".into(),
        Value::Object(merged.into_iter().map(|(k, (_, v))| (k, v)).collect()),
    );
    Ok(out)
}

fn localize_refs(value: &mut Value) {
    match value {
        Value::Object(map) => {
            for (key, v) in map.iter_mut() {
                match (key.as_str(), v) {
                    ("$ref", Value::String(r)) => {
                        if let Some((_, pointer)) = r.split_once('#') {
                            *r = format!("#{pointer}");
                        }
                    }
                    (_, v) => localize_refs(v),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(localize_refs),
        _ => {}
    }
}

/// Names of the definitions reachable from `roots` through local `$ref`s, including the roots.
pub fn closure(
    defs: &Map<String, Value>,
    roots: &[&str],
    stop_at: &[&str],
) -> Result<Vec<String>, BundleError> {
    let mut seen: Vec<String> = Vec::new();
    let mut stack: Vec<String> = roots.iter().map(|r| (*r).to_string()).collect();
    while let Some(name) = stack.pop() {
        if seen.contains(&name) || stop_at.contains(&name.as_str()) {
            continue;
        }
        let def = defs
            .get(&name)
            .ok_or_else(|| BundleError::UnknownDef { name: name.clone() })?;
        seen.push(name);
        collect_refs(def, &mut stack);
    }
    seen.sort();
    Ok(seen)
}

fn collect_refs(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                match (key.as_str(), v) {
                    ("$ref", Value::String(r)) => {
                        if let Some(name) = r.strip_prefix("#/$defs/") {
                            out.push(name.to_string());
                        }
                    }
                    (_, v) => collect_refs(v, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| collect_refs(v, out)),
        _ => {}
    }
}
