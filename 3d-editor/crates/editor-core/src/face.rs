use glam::DVec2;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FaceAttrs {
    pub material: String,
    #[schemars(with = "[f64; 2]")]
    pub offset: DVec2,
    #[schemars(with = "[f64; 2]")]
    pub scale: DVec2,
    pub rotation: f64,
}

impl Default for FaceAttrs {
    fn default() -> Self {
        Self {
            material: String::new(),
            offset: DVec2::ZERO,
            scale: DVec2::ONE,
            rotation: 0.0,
        }
    }
}

impl FaceAttrs {
    pub fn with_material(material: impl Into<String>) -> Self {
        Self {
            material: material.into(),
            ..Self::default()
        }
    }
}
