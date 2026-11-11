use std::collections::BTreeMap;

use editor_core::{ObjectId, ObjectKind};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    /// A stable colour per material name.
    #[default]
    Material,
    /// Faces facing up, facing down and vertical get three fixed colours.
    Orientation,
    /// A colour per layer.
    Layer,
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
    /// Objects, groups, brush entities or layers to leave out of the image. The document is not changed.
    #[serde(default)]
    pub hide: Vec<ObjectId>,
    /// Draw objects that are hidden in the document too (default false).
    #[serde(default)]
    pub show_hidden: bool,
    /// Draw nothing above this world z, for plan and section views.
    pub clip_above: Option<f64>,
    /// With the `labels` overlay, only label these objects (groups, brush entities and layers expand to their contents).
    #[serde(default)]
    pub label_ids: Vec<ObjectId>,
    /// With the `labels` overlay, only label these kinds: brush, mesh and/or entity (default all).
    #[serde(default)]
    pub label_kinds: Vec<ObjectKind>,
    /// Smallest visible area, in pixels, an object needs to get a label (default 12).
    pub min_label_pixels: Option<u32>,
    /// Most labels to draw (default 40).
    pub max_labels: Option<usize>,
    #[serde(default)]
    pub colors: ColorMode,
    /// Colour overrides by material name, as "#rrggbb"; they apply in every colour mode.
    #[serde(default)]
    pub material_colors: BTreeMap<String, String>,
    /// Also write the PNG to this path (needs mutate access).
    pub path: Option<String>,
}
