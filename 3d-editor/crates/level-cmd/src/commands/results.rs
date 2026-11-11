use std::borrow::Cow;
use std::collections::BTreeMap;

use glam::DVec3;
use level_core::{ElementRef, GroupId, ObjectId};
use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Created {
    pub ids: Vec<ObjectId>,
    /// The first of `ids`, when there is one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
}

impl Created {
    pub fn new(ids: Vec<ObjectId>) -> Self {
        let id = ids.first().copied();
        Self { ids, id }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatedMapped {
    pub ids: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Each input object -> the objects that replaced it.
    pub map: BTreeMap<ObjectId, Vec<ObjectId>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Duplicated {
    pub ids: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Each original -> its copy.
    pub map: BTreeMap<ObjectId, ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Arrayed {
    pub ids: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// Each original -> its copies.
    pub map: BTreeMap<ObjectId, Vec<ObjectId>>,
    /// How many objects one copy of the source holds.
    pub copy_size: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoomCreated {
    pub ids: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    /// The new wrapping group, when `name` was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupId>,
    /// floor, ceiling, wall_north, wall_south, wall_east, wall_west -> brush id.
    pub roles: BTreeMap<String, ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StairsCreated {
    pub ids: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupId>,
    /// The landing point at the top of the stairs.
    #[schemars(with = "[f64; 3]")]
    pub top: DVec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdCount {
    pub ids: Vec<ObjectId>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IdOnly {
    pub id: ObjectId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Updated {
    pub updated: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FacesChanged {
    pub faces: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct VerticesMoved {
    pub vertices: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FacesRemoved {
    pub removed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Transformed {
    pub transformed: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Deleted {
    pub deleted: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerDeleted {
    pub deleted: usize,
    pub moved: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Moved {
    pub moved: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ungrouped {
    pub ungrouped: usize,
    pub released: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Released {
    pub released: usize,
    pub deleted_entities: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundsBox {
    #[schemars(with = "[f64; 3]")]
    pub min: DVec3,
    #[schemars(with = "[f64; 3]")]
    pub max: DVec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectBounds {
    pub id: ObjectId,
    pub bounds: Option<BoundsBox>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoundsResult {
    pub bounds: Vec<ObjectBounds>,
    pub combined: Option<BoundsBox>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub brush: usize,
    pub mesh: usize,
    pub entity: usize,
    pub group: usize,
    pub layer: usize,
    pub camera: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayerRow {
    pub id: ObjectId,
    pub name: String,
    pub objects: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Summary {
    pub name: String,
    pub counts: Counts,
    pub bounds: Option<BoundsBox>,
    pub layers: Vec<LayerRow>,
    pub classnames: BTreeMap<String, usize>,
    pub selection: usize,
}

/// A description of one object: a JSON object that always has `id` and `kind`; the other keys depend on the requested detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ObjectView(pub Map<String, Value>);

impl JsonSchema for ObjectView {
    fn schema_name() -> Cow<'static, str> {
        "ObjectView".into()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let id = generator.subschema_for::<ObjectId>();
        let bounds = generator.subschema_for::<BoundsBox>();
        json_schema!({
            "description": "An object description; `id` is always present, other keys depend on the object kind and the requested detail.",
            "type": "object",
            "properties": {
                "id": id,
                "kind": { "type": "string" },
                "bounds": bounds,
            },
            "required": ["id"],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Objects {
    pub objects: Vec<ObjectView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectPage {
    pub objects: Vec<ObjectView>,
    pub next_cursor: Option<ObjectId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RayHit {
    pub id: ObjectId,
    /// `b12/f3`-style face reference, when a face was hit.
    pub face: Option<String>,
    #[schemars(with = "[f64; 3]")]
    pub point: DVec3,
    #[schemars(with = "[f64; 3]")]
    pub normal: DVec3,
    pub distance: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Raycast {
    pub hit: Option<RayHit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OverlapPair {
    pub a: ObjectId,
    pub b: ObjectId,
    /// True when only bounds were compared.
    pub approx: bool,
    pub depth: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Overlaps {
    pub pairs: Vec<OverlapPair>,
    pub count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IssueView {
    pub code: String,
    /// `error` or `warning`.
    pub severity: String,
    pub ids: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub elements: Vec<ElementRef>,
    pub message: String,
    pub hint: String,
}

impl From<super::check::Issue> for IssueView {
    fn from(i: super::check::Issue) -> Self {
        Self {
            code: i.code.into(),
            severity: i.severity.into(),
            ids: i.ids,
            elements: i.elements,
            message: i.message,
            hint: i.hint.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Validation {
    pub issues: Vec<IssueView>,
    pub errors: usize,
    pub warnings: usize,
}

/// A camera as returned by the view ops: the document camera fields, with `id` null and `session` true for viewport cameras.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CameraView(pub Map<String, Value>);

impl JsonSchema for CameraView {
    fn schema_name() -> Cow<'static, str> {
        "CameraView".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        json_schema!({
            "description": "A camera: id (null for viewport cameras), name, position, orientation and projection fields.",
            "type": "object",
            "properties": {
                "id": { "type": ["string", "null"] },
                "name": { "type": "string" },
                "session": { "type": "boolean" },
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CameraList {
    pub cameras: Vec<CameraView>,
    pub viewports: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CameraOne {
    pub camera: CameraView,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Framed {
    pub id: ObjectId,
    pub camera: CameraView,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BatchResults {
    /// One entry per item, in order: that item's `result`.
    pub results: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileInfoResult {
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Saved {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stepped {
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryRow {
    pub revision: u64,
    pub label: String,
    pub client: String,
    pub created: usize,
    pub modified: usize,
    pub deleted: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryList {
    pub entries: Vec<HistoryRow>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub open_transaction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClientRow {
    pub conn: u64,
    pub client: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionRow {
    pub id: String,
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
    pub revision: u64,
    pub focused: bool,
    pub clients: Vec<ClientRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionList {
    pub sessions: Vec<SessionRow>,
    pub focused: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SessionAttached {
    pub id: String,
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
    pub revision: u64,
    pub focused: bool,
    pub clients: Vec<ClientRow>,
    /// True when this call opened a new, empty session.
    pub created: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotLabel {
    pub id: ObjectId,
    pub x: u32,
    pub y: u32,
    pub pixels: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotResult {
    pub width: u32,
    pub height: u32,
    pub camera: CameraView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels: Option<Vec<ScreenshotLabel>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels_drawn: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub labels_omitted: Option<usize>,
    pub legend: Value,
    /// Where the PNG was written, when `path` was given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}
