use std::{collections::HashMap, sync::LazyLock};

use serde::{Serialize, de::DeserializeOwned};

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

pub trait Schema: Serialize + DeserializeOwned {
    const NAME: &'static str;
    const SCHEMA_JSON: &'static str;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub instance_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Toml,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Format::Json => f.write_str("JSON"),
            Format::Toml => f.write_str("TOML"),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaError {
    #[error("invalid {format}: {message}")]
    Parse { format: Format, message: String },
    #[error("{schema} failed validation: {}", format_issues(.errors))]
    Validation {
        schema: &'static str,
        errors: Vec<ValidationIssue>,
    },
    #[error("{schema} could not be converted: {source}")]
    Convert {
        schema: &'static str,
        source: serde_json::Error,
    },
}

fn format_issues(issues: &[ValidationIssue]) -> String {
    issues
        .iter()
        .map(|i| {
            if i.instance_path.is_empty() {
                i.message.clone()
            } else {
                format!("{}: {}", i.instance_path, i.message)
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

static VALIDATORS: LazyLock<HashMap<&'static str, jsonschema::Validator>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    register_all(&mut map);
    map
});

fn register<T: Schema>(map: &mut HashMap<&'static str, jsonschema::Validator>) {
    let schema: serde_json::Value = serde_json::from_str(T::SCHEMA_JSON).expect("embedded schema is valid JSON");
    let validator = jsonschema::validator_for(&schema).expect("embedded schema compiles");
    map.insert(T::NAME, validator);
}

fn validate<T: Schema>(value: &serde_json::Value) -> Result<(), SchemaError> {
    let validator = VALIDATORS.get(T::NAME).expect("schema registered");
    let errors: Vec<ValidationIssue> = validator
        .iter_errors(value)
        .map(|e| ValidationIssue {
            instance_path: e.instance_path().to_string(),
            message: e.to_string(),
        })
        .collect();
    if errors.is_empty() {
        Ok(())
    } else {
        Err(SchemaError::Validation { schema: T::NAME, errors })
    }
}

pub fn from_value<T: Schema>(value: serde_json::Value) -> Result<T, SchemaError> {
    validate::<T>(&value)?;
    serde_json::from_value(value).map_err(|source| SchemaError::Convert { schema: T::NAME, source })
}

pub fn to_value<T: Schema>(value: &T) -> Result<serde_json::Value, SchemaError> {
    let json = serde_json::to_value(value).map_err(|source| SchemaError::Convert { schema: T::NAME, source })?;
    validate::<T>(&json)?;
    Ok(json)
}

pub fn from_json_str<T: Schema>(text: &str) -> Result<T, SchemaError> {
    let value = serde_json::from_str(text).map_err(|e| SchemaError::Parse {
        format: Format::Json,
        message: e.to_string(),
    })?;
    from_value(value)
}

pub fn to_json_string_pretty<T: Schema>(value: &T) -> Result<String, SchemaError> {
    let json = to_value(value)?;
    serde_json::to_string_pretty(&json).map_err(|source| SchemaError::Convert { schema: T::NAME, source })
}

pub fn from_toml_str<T: Schema>(text: &str) -> Result<T, SchemaError> {
    let value: serde_json::Value = toml::from_str(text).map_err(|e| SchemaError::Parse {
        format: Format::Toml,
        message: e.to_string(),
    })?;
    from_value(value)
}

fn merge(base: &mut serde_json::Value, over: serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(base), serde_json::Value::Object(over)) => {
            for (key, value) in over {
                match base.get_mut(&key) {
                    Some(slot) => merge(slot, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (slot, over) => *slot = over,
    }
}

pub fn from_toml_layers<T: Schema>(layers: &[&str]) -> Result<T, SchemaError> {
    let mut merged = serde_json::Value::Object(Default::default());
    for text in layers {
        let layer: serde_json::Value = toml::from_str(text).map_err(|e| SchemaError::Parse {
            format: Format::Toml,
            message: e.to_string(),
        })?;
        merge(&mut merged, layer);
    }
    from_value(merged)
}
