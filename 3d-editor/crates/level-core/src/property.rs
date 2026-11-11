use std::collections::BTreeMap;

use glam::DVec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Bool(bool),
    Number(f64),
    Vec3(DVec3),
    String(String),
}

pub type Properties = BTreeMap<String, PropertyValue>;

pub type Extra = BTreeMap<String, toml::Value>;

impl schemars::JsonSchema for PropertyValue {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "PropertyValue".into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "anyOf": [
                { "type": "boolean" },
                { "type": "number" },
                { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 },
                { "type": "string" },
            ],
            "description": "bool, number, [x, y, z] or string",
        })
    }
}

pub fn extra_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "object",
        "description": "Unrecognized keys preserved from the level file.",
    })
}
