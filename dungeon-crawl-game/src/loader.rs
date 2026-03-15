use crate::schema_types::*;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
    Json(serde_json::Error),
    Schema(String),
    Validation(Vec<String>),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "IO error: {e}"),
            LoadError::Yaml(e) => write!(f, "YAML parse error: {e}"),
            LoadError::Json(e) => write!(f, "JSON error: {e}"),
            LoadError::Schema(e) => write!(f, "Schema compilation error: {e}"),
            LoadError::Validation(errors) => {
                writeln!(f, "Validation errors:")?;
                for e in errors {
                    writeln!(f, "  - {e}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self { LoadError::Io(e) }
}
impl From<serde_yaml::Error> for LoadError {
    fn from(e: serde_yaml::Error) -> Self { LoadError::Yaml(e) }
}
impl From<serde_json::Error> for LoadError {
    fn from(e: serde_json::Error) -> Self { LoadError::Json(e) }
}

fn build_resolver() -> HashMap<String, Value> {
    let mut map = HashMap::new();
    let schemas: &[(&str, &str)] = &[
        ("effect.json", SCHEMA_EFFECT),
        ("config.json", SCHEMA_CONFIG),
        ("player.json", SCHEMA_PLAYER),
        ("room.json", SCHEMA_ROOM),
        ("item.json", SCHEMA_ITEM),
        ("generator_context.json", SCHEMA_GENERATOR_CONTEXT),
    ];
    for (name, raw) in schemas {
        let val: Value = serde_json::from_str(raw).unwrap();
        map.insert(name.to_string(), val);
    }
    map
}

fn resolve_refs(schema: &mut Value, resolver: &HashMap<String, Value>) {
    match schema {
        Value::Object(map) => {
            if let Some(Value::String(ref_str)) = map.get("$ref") {
                if let Some(idx) = ref_str.find('#') {
                    let file = &ref_str[..idx];
                    let pointer = ref_str[idx + 1..].to_string();
                    if !file.is_empty() {
                        if let Some(external) = resolver.get(file) {
                            if let Some(resolved) = external.pointer(&pointer) {
                                let mut resolved = resolved.clone();
                                resolve_refs(&mut resolved, resolver);
                                *schema = resolved;
                                return;
                            }
                        }
                    }
                }
            }
            for val in map.values_mut() {
                resolve_refs(val, resolver);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                resolve_refs(val, resolver);
            }
        }
        _ => {}
    }
}

fn validate_against_schema(data: &Value, schema_raw: &str) -> Result<(), LoadError> {
    let resolver = build_resolver();
    let mut schema: Value = serde_json::from_str(schema_raw)?;
    resolve_refs(&mut schema, &resolver);

    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft7)
        .build(&schema)
        .map_err(|e| LoadError::Schema(e.to_string()))?;

    let result = validator.validate(data);
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            let msg = e.to_string();
            Err(LoadError::Validation(vec![msg]))
        }
    }
}

pub fn load_config(path: impl AsRef<Path>) -> Result<Config, LoadError> {
    let content = fs::read_to_string(path)?;
    let value: Value = serde_yaml::from_str(&content)?;
    validate_against_schema(&value, SCHEMA_CONFIG)?;
    let config: Config = serde_json::from_value(value)?;
    Ok(config)
}

pub fn load_theme(path: impl AsRef<Path>) -> Result<Vec<GeneratorContext>, LoadError> {
    let content = fs::read_to_string(path)?;
    let value: Value = serde_yaml::from_str(&content)?;

    let arr = value
        .as_array()
        .ok_or_else(|| LoadError::Validation(vec!["theme must be an array".to_string()]))?;

    for (i, item) in arr.iter().enumerate() {
        validate_against_schema(item, SCHEMA_GENERATOR_CONTEXT)
            .map_err(|e| match e {
                LoadError::Validation(msgs) => LoadError::Validation(
                    msgs.into_iter()
                        .map(|m| format!("[{i}] {m}"))
                        .collect(),
                ),
                other => other,
            })?;
    }

    let themes: Vec<GeneratorContext> = serde_json::from_value(value)?;
    Ok(themes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_refs_inlines_external() {
        let mut resolver = HashMap::new();
        resolver.insert(
            "effect.json".to_string(),
            serde_json::json!({
                "definitions": {
                    "Foo": { "type": "string" }
                }
            }),
        );
        let mut schema = serde_json::json!({
            "properties": {
                "x": { "$ref": "effect.json#/definitions/Foo" }
            }
        });
        resolve_refs(&mut schema, &resolver);
        assert_eq!(schema["properties"]["x"]["type"], "string");
    }

    #[test]
    fn resolve_refs_no_ref_unchanged() {
        let resolver = HashMap::new();
        let mut schema = serde_json::json!({ "type": "string" });
        let original = schema.clone();
        resolve_refs(&mut schema, &resolver);
        assert_eq!(schema, original);
    }

    #[test]
    fn validate_valid_config() {
        let data = serde_json::json!({ "model": "llama3" });
        assert!(validate_against_schema(&data, SCHEMA_CONFIG).is_ok());
    }

    #[test]
    fn validate_invalid_config_missing_model() {
        let data = serde_json::json!({});
        assert!(validate_against_schema(&data, SCHEMA_CONFIG).is_err());
    }

    #[test]
    fn validate_valid_player() {
        let data = serde_json::json!({
            "name": "Test",
            "description": "A test player",
            "stats": { "strength": 10, "speed": 10, "intelligence": 10, "sanity": 10 }
        });
        assert!(validate_against_schema(&data, SCHEMA_PLAYER).is_ok());
    }
}
