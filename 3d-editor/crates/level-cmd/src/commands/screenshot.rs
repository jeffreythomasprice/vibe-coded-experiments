use level_core::ObjectId;
use schemars::JsonSchema;
use serde::Deserialize;

use super::view::CameraParams;

#[derive(Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum CameraArg {
    Reference(String),
    Inline(CameraParams),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Shaded,
    Flat,
    Wireframe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Overlay {
    Grid,
    Axes,
    Labels,
    Bounds,
    Selection,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotArgs {
    pub camera: Option<CameraArg>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    #[serde(default)]
    pub mode: Mode,
    #[serde(default)]
    pub overlays: Vec<Overlay>,
    #[serde(default)]
    pub highlight: Vec<ObjectId>,
}
