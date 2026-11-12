use crate::{ComponentDefsId, Extra};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub const DEFS_VERSION: u32 = 2;

mod generated {
    include!(concat!(env!("OUT_DIR"), "/component_defs.rs"));
}

pub use generated::{ComponentDef, ComponentDefName, ComponentDefsFile, ComponentSize};

/// Where a set of component definitions comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum DefsSource {
    /// Absolute, or relative to the directory of the scene file.
    Path { path: String },
    /// The definitions themselves, stored in the scene.
    Embedded { defs: ComponentDefsFile },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ComponentDefs {
    pub id: ComponentDefsId,
    pub source: DefsSource,
    #[schemars(schema_with = "crate::property::extra_schema")]
    pub extra: Extra,
}
