use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hearthstone {
    pub name: String,
    pub level: u8,
    #[serde(default)]
    pub aspect: String,
    #[serde(default)]
    pub description: String,
}
