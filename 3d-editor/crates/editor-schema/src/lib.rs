mod bundle;
mod error;
mod sources;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};

pub use bundle::{DRAFT, bundle, closure};
pub use error::{BundleError, ParseError, SchemaCompileError, Violation};
pub use sources::FILES;

/// A type generated from a definition in `schemas/`. `NAME` is the definition's name, unique across all files.
pub trait Schema: DeserializeOwned {
    const NAME: &'static str;
}

/// Every committed schema file merged into one document with a single `$defs`.
pub fn bundled() -> &'static Map<String, Value> {
    static BUNDLE: OnceLock<Map<String, Value>> = OnceLock::new();
    BUNDLE.get_or_init(|| bundle(FILES).expect("committed schemas bundle"))
}

/// The standalone schema of one definition: a `$ref` to it beside the shared `$defs`.
pub fn schema_value(name: &str) -> Result<Value, BundleError> {
    let all = bundled();
    if !all["$defs"]
        .as_object()
        .is_some_and(|defs| defs.contains_key(name))
    {
        return Err(BundleError::UnknownDef {
            name: name.to_string(),
        });
    }
    let mut out = Map::new();
    out.insert("$schema".into(), Value::String(DRAFT.into()));
    out.insert("$ref".into(), Value::String(format!("#/$defs/{name}")));
    out.insert("$defs".into(), all["$defs"].clone());
    Ok(Value::Object(out))
}

fn violations(validator: &jsonschema::Validator, instance: &Value) -> Vec<Violation> {
    validator
        .iter_errors(instance)
        .map(|e| Violation {
            pointer: e.instance_path().to_string(),
            schema_pointer: e.schema_path().to_string(),
            message: e.to_string(),
        })
        .collect()
}

fn validator_for_def(name: &'static str) -> Arc<jsonschema::Validator> {
    static CACHE: OnceLock<Mutex<HashMap<&'static str, Arc<jsonschema::Validator>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    cache
        .entry(name)
        .or_insert_with(|| {
            let schema = schema_value(name).expect("generated type names a committed definition");
            Arc::new(jsonschema::validator_for(&schema).expect("committed schemas compile"))
        })
        .clone()
}

/// Checks `value` against the schema of `T`, then builds the `T`.
pub fn parse<T: Schema>(value: &Value) -> Result<T, ParseError> {
    let found = violations(&validator_for_def(T::NAME), value);
    if !found.is_empty() {
        return Err(ParseError::Invalid(found));
    }
    serde_path_to_error::deserialize(value).map_err(|e| ParseError::Deserialize {
        pointer: pointer_of(e.path()),
        message: e.inner().to_string(),
    })
}

pub fn parse_str<T: Schema>(text: &str) -> Result<T, ParseError> {
    parse(&parse_json(text)?)
}

pub fn parse_slice<T: Schema>(bytes: &[u8]) -> Result<T, ParseError> {
    let value = serde_json::from_slice(bytes).map_err(|e| ParseError::Syntax {
        message: e.to_string(),
    })?;
    parse(&value)
}

fn parse_json(text: &str) -> Result<Value, ParseError> {
    serde_json::from_str(text).map_err(|e| ParseError::Syntax {
        message: e.to_string(),
    })
}

fn pointer_of(path: &serde_path_to_error::Path) -> String {
    use serde_path_to_error::Segment;
    path.iter()
        .map(|s| match s {
            Segment::Seq { index } => format!("/{index}"),
            Segment::Map { key } => format!("/{}", key.replace('~', "~0").replace('/', "~1")),
            Segment::Enum { variant } => format!("/{variant}"),
            Segment::Unknown => String::new(),
        })
        .collect()
}

/// A user-authored schema (an entity class's property schema), compiled once and reused.
pub struct CompiledSchema(jsonschema::Validator);

impl CompiledSchema {
    pub fn compile(schema: &Value) -> Result<Self, SchemaCompileError> {
        jsonschema::validator_for(schema)
            .map(Self)
            .map_err(|e| SchemaCompileError::Invalid(e.to_string()))
    }

    pub fn validate(&self, instance: &Value) -> Vec<Violation> {
        violations(&self.0, instance)
    }
}

/// One-off check against a schema that is not in `schemas/`.
pub fn validate_dynamic(
    schema: &Value,
    instance: &Value,
) -> Result<Vec<Violation>, SchemaCompileError> {
    CompiledSchema::compile(schema).map(|c| c.validate(instance))
}
