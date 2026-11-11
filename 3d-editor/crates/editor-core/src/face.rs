use glam::{DVec2, DVec3};
use serde::{Deserialize, Serialize};

/// Explicit texture axes. Without them the axes come from the face normal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UvAxes {
    #[schemars(with = "[f64; 3]")]
    pub u: DVec3,
    #[schemars(with = "[f64; 3]")]
    pub v: DVec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FaceAttrs {
    pub material: String,
    /// In texels.
    #[schemars(with = "[f64; 2]")]
    pub offset: DVec2,
    /// World units per texel.
    #[schemars(with = "[f64; 2]")]
    pub scale: DVec2,
    /// Degrees, about the face normal.
    pub rotation: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<UvAxes>,
}

impl Default for FaceAttrs {
    fn default() -> Self {
        Self {
            material: String::new(),
            offset: DVec2::ZERO,
            scale: DVec2::ONE,
            rotation: 0.0,
            axes: None,
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
