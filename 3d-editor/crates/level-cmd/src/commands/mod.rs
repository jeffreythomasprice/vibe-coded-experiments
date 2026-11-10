mod brush;
mod build;
mod check;
mod csg;
mod entity;
mod face;
mod group;
mod hints;
mod layer;
mod mesh;
mod object;
mod scene;
pub mod screenshot;
pub(crate) mod session_args;
mod transform;
pub mod util;
pub mod view;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use level_core::ObjectId;

use crate::batch;
use crate::ctx::Ctx;
use crate::error::CommandError;

pub type CmdResult = Result<Outcome, CommandError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Safety {
    Read,
    Mutate,
    Destructive,
}

impl Safety {
    pub fn as_str(self) -> &'static str {
        match self {
            Safety::Read => "read",
            Safety::Mutate => "mutate",
            Safety::Destructive => "destructive",
        }
    }
}

/// Where an op runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exec {
    /// `dispatch` through a `Ctx`: batchable and dry-runnable.
    Document,
    /// Handled by `Session` itself (history, selection, files).
    Session,
    /// Needs a renderer, so the client handles it.
    Client,
}

#[derive(Clone, Copy)]
pub struct OpInfo {
    pub name: &'static str,
    pub safety: Safety,
    pub exec: Exec,
    pub summary: &'static str,
    pub args_schema: fn() -> schemars::Schema,
}

impl OpInfo {
    pub fn undoable(&self) -> bool {
        self.exec == Exec::Document && self.safety != Safety::Read
    }

    pub fn find(name: &str) -> Option<&'static OpInfo> {
        OPS.iter()
            .find(|o| o.name == name)
            .or_else(|| OPS.iter().find(|o| o.tool_name() == name))
    }

    pub fn tool_name(&self) -> String {
        self.name.replace('.', "_")
    }
}

impl std::fmt::Debug for OpInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpInfo")
            .field("name", &self.name)
            .field("safety", &self.safety)
            .field("exec", &self.exec)
            .finish()
    }
}

fn schema_of<T: JsonSchema>() -> schemars::Schema {
    schemars::schema_for!(T)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArgs {}

fn unavailable(op: &str) -> CommandError {
    match OpInfo::find(op) {
        Some(info) if info.exec == Exec::Client => CommandError::invalid_arg(
            "op",
            format!("{op} needs a renderer and is handled by the client"),
        )
        .with_hint("use `level-cli <file> screenshot`, or level_render::screenshot from Rust"),
        Some(_) => CommandError::invalid_arg(
            "op",
            format!("{op} is a session operation and cannot run here"),
        )
        .with_hint("send it directly, not inside a batch"),
        None => CommandError::invalid_arg("op", format!("unknown op `{op}`")).with_hint(format!(
            "available ops: {}",
            OPS.iter().map(|o| o.name).collect::<Vec<_>>().join(", ")
        )),
    }
}

macro_rules! ops {
    (
        document {
            $( $name:literal $safety:ident $args:ty => $handler:path, $summary:literal; )*
        }
        session {
            $( $sname:literal $ssafety:ident $sargs:ty, $ssummary:literal; )*
        }
        client {
            $( $cname:literal $csafety:ident $cargs:ty, $csummary:literal; )*
        }
    ) => {
        pub const OPS: &[OpInfo] = &[
            $( OpInfo { name: $name, safety: Safety::$safety, exec: Exec::Document, summary: $summary, args_schema: schema_of::<$args> }, )*
            $( OpInfo { name: $sname, safety: Safety::$ssafety, exec: Exec::Session, summary: $ssummary, args_schema: schema_of::<$sargs> }, )*
            $( OpInfo { name: $cname, safety: Safety::$csafety, exec: Exec::Client, summary: $csummary, args_schema: schema_of::<$cargs> }, )*
        ];

        pub fn dispatch(ctx: &mut Ctx, op: &str, args: Value) -> CmdResult {
            let op = OpInfo::find(op).map_or(op, |o| o.name);
            match op {
                $( $name => $handler(ctx, parse_op_args(op, args)?), )*
                _ => Err(unavailable(op)),
            }
        }
    };
}

ops! {
    document {
        "brush.create_box" Mutate brush::CreateBox => brush::create_box, "Create an axis-aligned box brush from min and max corners.";
        "brush.create_wedge" Mutate brush::CreateWedge => brush::create_wedge, "Create a ramp as one convex brush: the box min..max rising from zero height to full height toward a direction (+x, -x, +y, -y).";
        "brush.create_prism" Mutate brush::CreatePrism => brush::create_prism, "Create a regular n-sided prism brush (cylinders, wedges) along an axis.";
        "brush.create_from_planes" Mutate brush::CreateFromPlanes => brush::create_from_planes, "Create a convex brush from face planes, each given by three points.";
        "brush.clip" Mutate brush::Clip => brush::clip, "Clip brushes by a plane given as three points, keeping the front side, back side or both. result.map lists each brush -> resulting ids.";
        "brush.to_mesh" Mutate mesh::Convert => mesh::brush_to_mesh_cmd, "Convert brushes to meshes (new ids, same parents).";
        "mesh.create" Mutate mesh::Create => mesh::create, "Create a polygon mesh from vertices and face loops (counter-clockwise from outside).";
        "mesh.to_brush" Mutate mesh::Convert => mesh::mesh_to_brush_cmd, "Convert closed convex meshes to brushes (new ids, same parents).";
        "mesh.delete_faces" Mutate mesh::DeleteFaces => mesh::delete_faces, "Remove faces from meshes; vertices no face uses are dropped.";
        "face.set_material" Mutate face::SetMaterial => face::set_material, "Set the material of faces (b12/f3), or of every face of an object (b12).";
        "face.set_uv" Mutate face::SetUv => face::set_uv, "Set face UV offset, scale and rotation; omitted fields are kept.";
        "face.translate" Mutate face::TranslateFaces => face::translate_faces, "Move the vertices of mesh faces by a delta.";
        "vertex.translate" Mutate face::TranslateVertices => face::translate_vertices, "Move mesh vertices (m3/v10) by a delta.";
        "room.create" Mutate build::RoomCreate => build::room_create, "Create a hollow room: floor, ceiling and four wall brushes around the interior min..max. result.roles maps floor, ceiling, wall_north/south/east/west to ids.";
        "room.cut_opening" Mutate build::CutOpening => build::cut_opening, "Cut a box-shaped opening (doorway, window, corridor mouth) through every brush it crosses. The cutter never enters the document; result.map lists target -> resulting ids.";
        "stairs.create" Mutate build::StairsCreate => build::stairs_create, "Create stairs as non-overlapping step brushes, each solid from the floor up. result.ids run bottom to top; result.top is the landing point.";
        "csg.subtract" Mutate csg::Subtract => csg::subtract, "Subtract cutter brushes or meshes from targets. result.map lists each target -> resulting ids; consume_cutters deletes the cutters too (destructive).";
        "transform.translate" Mutate transform::Translate => transform::translate, "Move objects by a delta. Groups and brush entities move their contents.";
        "transform.rotate" Mutate transform::Rotate => transform::rotate, "Rotate objects about an axis (right-hand rule, degrees) around a pivot: center, origin or a point.";
        "transform.scale" Mutate transform::Scale => transform::scale, "Scale objects by a factor or per-axis factors around a pivot; negative factors mirror.";
        "transform.flip" Mutate transform::Flip => transform::flip, "Mirror objects across the plane through the pivot whose normal is the axis.";
        "object.delete" Destructive object::Delete => object::delete, "Delete objects and everything they own. Undoable.";
        "object.duplicate" Mutate object::Duplicate => object::duplicate, "Deep-copy objects (with group and entity contents), optionally offset. result.map lists old -> new ids.";
        "object.array" Mutate object::Array => object::array, "Make count extra deep copies at multiples of an offset, optionally with a mirrored copy of the whole row, as one step. result.map lists old -> new ids.";
        "object.rename" Mutate object::Rename => object::rename, "Rename a group, layer or camera.";
        "object.set_layer" Mutate object::SetLayer => object::set_layer, "Move objects, with their contents, to another layer.";
        "entity.create" Mutate entity::Create => entity::create, "Create a point entity with classname, origin, angles and properties.";
        "entity.create_brush_entity" Mutate entity::CreateBrushEntity => entity::create_brush_entity, "Make a brush entity that owns the given brushes and meshes.";
        "entity.release_objects" Mutate entity::Release => entity::release_objects, "Return brushes and meshes to the world; an emptied brush entity is deleted.";
        "entity.set_properties" Mutate entity::SetProperties => entity::set_properties, "Set and remove key/value properties on entities.";
        "entity.set_classname" Mutate entity::SetClassname => entity::set_classname, "Change the classname of entities.";
        "entity.set_transform" Mutate entity::SetTransform => entity::set_transform, "Set a point entity's origin and/or angles (pitch, yaw, roll) absolutely.";
        "layer.create" Mutate layer::Create => layer::create, "Create a layer.";
        "layer.delete" Destructive layer::Delete => layer::delete, "Delete a layer, moving its contents to move_to if it has any.";
        "group.create" Mutate group::Create => group::create, "Group objects that share a layer and parent group.";
        "group.ungroup" Mutate group::Ungroup => group::ungroup, "Dissolve groups; their contents go to the group's parent.";
        "group.move" Mutate group::Move => group::move_objects, "Move objects into a group, or out of any group when group is null.";
        "scene.summary" Read NoArgs => scene::summary, "Compact overview: counts by kind, bounds, layers, entity classnames and selection size.";
        "scene.list" Read scene::List => scene::list, "Filter and page through objects by kind, layer, group, entity, classname, name or region.";
        "scene.get" Read scene::Get => scene::get, "Details of objects by id, at summary, full or geometry level.";
        "scene.bounds" Read scene::Bounds => scene::bounds, "Axis-aligned bounds of objects, each and combined.";
        "scene.raycast" Read scene::Raycast => scene::raycast, "First brush, mesh or point entity hit by a ray: id, face, point, normal, distance.";
        "scene.query_box" Read scene::QueryBox => scene::query_box, "Objects whose bounds intersect or lie inside a box.";
        "scene.overlaps" Read scene::Overlaps => scene::overlaps, "Pairs of overlapping brushes and meshes (exact for brush pairs, bounds-only otherwise).";
        "check.validate" Read check::Validate => check::validate_cmd, "Run the issue checker and return structured issues.";
        "view.camera.list" Read NoArgs => view::list, "List document cameras and the session viewport cameras.";
        "view.camera.get" Read view::Get => view::get, "Resolve a camera by id, name or viewport.* name.";
        "view.camera.create" Mutate view::CreateCameraArgs => view::create, "Create a named perspective or orthographic camera.";
        "view.camera.set" Mutate view::SetCameraArgs => view::set, "Edit a document camera.";
        "view.camera.delete" Destructive view::Delete => view::delete, "Delete document cameras.";
        "view.frame" Mutate view::Frame => view::frame, "Move a document camera so the given objects fill the image.";
        "batch" Mutate batch::Batch => batch::run, "Run commands as one all-or-nothing transaction. Items use dotted or underscored op names; name an item with `as` and reference results as $name, $N, $name.ids[k], $name/v2 or $name/f1.";
    }
    session {
        "history.begin" Mutate session_args::Begin, "Open a transaction; edits group into one undo step until commit or rollback.";
        "history.commit" Mutate NoArgs, "Commit the open transaction as one undo step.";
        "history.rollback" Mutate NoArgs, "Discard the open transaction.";
        "history.undo" Destructive NoArgs, "Undo the last committed step.";
        "history.redo" Mutate NoArgs, "Redo the last undone step.";
        "history.list" Read session_args::List, "List recent history entries with client attribution.";
        "select.get" Read NoArgs, "The current editor selection (shared with the GUI).";
        "select.set" Mutate session_args::SelectIds, "Replace the selection with these brushes, meshes, entities or groups.";
        "select.add" Mutate session_args::SelectIds, "Add objects to the selection.";
        "select.remove" Mutate session_args::SelectIds, "Remove objects from the selection.";
        "select.clear" Mutate NoArgs, "Clear the selection.";
        "select.all" Mutate session_args::SelectKind, "Select every selectable object, optionally of one kind.";
        "select.invert" Mutate session_args::SelectKind, "Select what is not selected (optionally within one kind) and deselect the rest.";
        "file.info" Read NoArgs, "Path (null until the document has a file), name, dirty flag and revision of the open document.";
        "file.save" Mutate session_args::Save, "Write the document to its file, or to `path` (which becomes the document path; creates parent dirs). Fails with FileExists for another existing file unless overwrite is true.";
        "file.new" Destructive session_args::New, "Replace the document with an empty one, optionally bound to a new `path` (nothing is written until file.save). Fails with UnsavedChanges unless discard is true.";
        "file.open" Destructive session_args::Open, "Load a level file, replacing the document. Fails with UnsavedChanges unless discard is true.";
    }
    client {
        "view.screenshot" Read screenshot::ScreenshotArgs, "Render a PNG from a camera with optional overlays (grid, axes, labels, bounds) and highlighted objects.";
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub result: Value,
    pub warnings: Vec<String>,
}

impl Outcome {
    pub fn new(result: Value) -> Self {
        Self {
            result,
            warnings: Vec::new(),
        }
    }

    pub fn with_map(mut self, map: Vec<(ObjectId, Vec<ObjectId>)>) -> Self {
        let map: serde_json::Map<String, Value> = map
            .into_iter()
            .map(|(from, to)| (from.to_string(), json!(to)))
            .collect();
        self.result["map"] = Value::Object(map);
        self
    }

    pub fn created(ids: Vec<ObjectId>, warnings: Vec<String>) -> Self {
        let mut result = json!({ "ids": ids });
        if let Some(first) = ids.first() {
            result["id"] = json!(first);
        }
        Self { result, warnings }
    }
}

fn parse_failure(
    e: serde_path_to_error::Error<serde_json::Error>,
) -> (CommandError, Vec<hints::Step>) {
    let steps = hints::path_steps(e.path());
    let path = e.path().to_string();
    let message = e.inner().to_string();
    let field = if path == "." {
        message.split('`').nth(1).map(str::to_string)
    } else {
        Some(path)
    };
    (
        CommandError::InvalidArgument {
            field,
            message,
            hint: None,
        },
        steps,
    )
}

fn deserialize<T: DeserializeOwned>(args: Value) -> Result<T, (CommandError, Vec<hints::Step>)> {
    let args = if args.is_null() { json!({}) } else { args };
    serde_path_to_error::deserialize(args).map_err(parse_failure)
}

pub fn parse_args<T: DeserializeOwned>(args: Value) -> Result<T, CommandError> {
    deserialize(args).map_err(|(e, _)| e)
}

fn parse_op_args<T: DeserializeOwned>(op: &str, args: Value) -> Result<T, CommandError> {
    deserialize(args).map_err(|(e, steps)| {
        let schema = OpInfo::find(op).and_then(|i| serde_json::to_value((i.args_schema)()).ok());
        match schema.and_then(|s| hints::describe(&s, &steps)) {
            Some(hint) => e.with_hint(hint),
            None => e,
        }
    })
}

/// The safety class an invocation needs: the op's own class, raised by arguments with side effects.
pub fn required_safety(op: &str, args: &Value) -> Option<Safety> {
    let info = OpInfo::find(op)?;
    let flag = |key: &str| {
        args.get(key)
            .is_some_and(|v| !v.is_null() && v != &json!(false))
    };
    Some(match info.name {
        "csg.subtract" if flag("consume_cutters") => Safety::Destructive,
        "view.screenshot" if flag("path") => info.safety.max(Safety::Mutate),
        _ => info.safety,
    })
}
