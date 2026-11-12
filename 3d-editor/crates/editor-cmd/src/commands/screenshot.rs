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
    /// Arrows between entities that reference each other (properties marked `editor.linkType = entityID`).
    Links,
}

/// Render the scene as the game client would at a given simulation time.
#[derive(Debug, Clone, Copy, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Simulate {
    /// Seconds since the simulation started; animations are sampled here. 0 is the rest pose.
    #[serde(default)]
    pub time: f64,
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
    /// Draw material images (default true). false draws every face as a flat colour.
    pub textures: Option<bool>,
    /// Draw the scene as played: animated entities posed at this time, no entity markers or edge lines, and hidden flags ignored (layers marked omit_from_export are left out).
    pub simulate: Option<Simulate>,
    /// Also write the PNG to this path (needs mutate access).
    pub path: Option<String>,
}
