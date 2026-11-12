use std::collections::BTreeMap;

pub use serde_json::Value;

/// Open JSON object. Its shape is defined by the component's schema, not by the editor.
/// `null` is not allowed anywhere inside (the scene format cannot store it); see `find_null`.
pub type Properties = serde_json::Map<String, Value>;

pub fn valid_component_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub type Extra = BTreeMap<String, toml::Value>;

/// JSON pointer of the first `null` inside `value`, if any.
pub fn find_null(value: &Value) -> Option<String> {
    fn walk(value: &Value, path: &mut String) -> bool {
        match value {
            Value::Null => true,
            Value::Array(items) => items.iter().enumerate().any(|(i, v)| {
                let len = path.len();
                path.push_str(&format!("/{i}"));
                let found = walk(v, path);
                if !found {
                    path.truncate(len);
                }
                found
            }),
            Value::Object(map) => map.iter().any(|(k, v)| {
                let len = path.len();
                path.push('/');
                path.push_str(&k.replace('~', "~0").replace('/', "~1"));
                let found = walk(v, path);
                if !found {
                    path.truncate(len);
                }
                found
            }),
            _ => false,
        }
    }
    let mut path = String::new();
    walk(value, &mut path).then_some(path)
}

pub fn extra_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "Unrecognized keys preserved from the scene file.",
    })
}
