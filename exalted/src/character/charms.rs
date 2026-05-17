use serde::{Deserialize, Serialize};

use super::traits::DotSource;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChosenCharm {
    pub name: String,
    pub source: DotSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl ChosenCharm {
    pub fn new(name: impl Into<String>, source: DotSource) -> Self {
        Self {
            name: name.into(),
            source,
            notes: None,
        }
    }
}
