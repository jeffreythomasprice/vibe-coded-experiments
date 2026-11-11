pub(crate) mod brush;
mod build;
mod check;
mod csg;
pub(crate) mod entity;
pub(crate) mod face;
mod group;
mod hints;
mod layer;
mod locks;
mod mesh;
pub(crate) mod object;
pub mod results;
pub(crate) mod scene;
pub mod screenshot;
pub(crate) mod session_args;
pub(crate) mod transform;
pub mod util;
pub mod view;
pub mod visibility;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use editor_core::ObjectId;

use crate::batch;
use crate::ctx::Ctx;
use crate::envelope::Success;
use crate::error::CommandError;

pub type CmdResult<R = Value> = Result<Outcome<R>, CommandError>;

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
    /// Handled by the server hub (sessions), not by any one session.
    Server,
}

#[derive(Clone, Copy)]
pub struct OpInfo {
    pub name: &'static str,
    pub safety: Safety,
    pub exec: Exec,
    pub summary: &'static str,
    pub args_schema: fn() -> schemars::Schema,
    pub result_schema: fn() -> schemars::Schema,
    pub response_schema: fn() -> schemars::Schema,
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

#[derive(Debug, Clone, Default, serde::Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoArgs {}

pub(crate) fn unavailable(op: &str) -> CommandError {
    match OpInfo::find(op) {
        Some(info) if info.exec == Exec::Client => CommandError::invalid_arg(
            "op",
            format!("{op} needs a renderer and is handled by the client"),
        )
        .with_hint("use `editor-cli <file> screenshot`, or editor_render::screenshot from Rust"),
        Some(info) if info.exec == Exec::Server => CommandError::invalid_arg(
            "op",
            format!("{op} is handled by the server and needs a connection to it"),
        )
        .with_hint("send it through editor-server (editor-cli, editor-mcp or the GUI)"),
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
            $( $name:literal $safety:ident $args:ty => $res:ty = $handler:path, $summary:literal; )*
        }
        session {
            $( $sname:literal $ssafety:ident $sargs:ty => $sres:ty, $ssummary:literal; )*
        }
        client {
            $( $cname:literal $csafety:ident $cargs:ty => $cres:ty, $csummary:literal; )*
        }
        server {
            $( $vname:literal $vsafety:ident $vargs:ty => $vres:ty, $vsummary:literal; )*
        }
    ) => {
        pub const OPS: &[OpInfo] = &[
            $( OpInfo { name: $name, safety: Safety::$safety, exec: Exec::Document, summary: $summary, args_schema: schema_of::<$args>, result_schema: schema_of::<$res>, response_schema: schema_of::<Success<$res>> }, )*
            $( OpInfo { name: $sname, safety: Safety::$ssafety, exec: Exec::Session, summary: $ssummary, args_schema: schema_of::<$sargs>, result_schema: schema_of::<$sres>, response_schema: schema_of::<Success<$sres>> }, )*
            $( OpInfo { name: $cname, safety: Safety::$csafety, exec: Exec::Client, summary: $csummary, args_schema: schema_of::<$cargs>, result_schema: schema_of::<$cres>, response_schema: schema_of::<Success<$cres>> }, )*
            $( OpInfo { name: $vname, safety: Safety::$vsafety, exec: Exec::Server, summary: $vsummary, args_schema: schema_of::<$vargs>, result_schema: schema_of::<$vres>, response_schema: schema_of::<Success<$vres>> }, )*
        ];

        fn dispatch_op(ctx: &mut Ctx, op: &str, args: Value) -> CmdResult {
            match op {
                $( $name => $handler(ctx, parse_op_args(op, args)?).and_then(Outcome::erase), )*
                _ => Err(unavailable(op)),
            }
        }
    };
}

ops! {
    document {
        "brush.create_box" Mutate brush::CreateBox => results::Created = brush::create_box, "Create an axis-aligned box brush from min and max corners.";
        "brush.create_wedge" Mutate brush::CreateWedge => results::Created = brush::create_wedge, "Create a ramp as one convex brush: the box min..max rising from zero height to full height toward a direction (+x, -x, +y, -y).";
        "brush.create_prism" Mutate brush::CreatePrism => results::Created = brush::create_prism, "Create a regular n-sided prism brush (cylinders, wedges) along an axis.";
        "brush.create_from_planes" Mutate brush::CreateFromPlanes => results::Created = brush::create_from_planes, "Create a convex brush from face planes, each given by three points.";
        "brush.clip" Mutate brush::Clip => results::CreatedMapped = brush::clip, "Clip brushes by a plane given as three points, keeping the front side, back side or both. result.map lists each brush -> resulting ids.";
        "brush.to_mesh" Mutate mesh::Convert => results::CreatedMapped = mesh::brush_to_mesh_cmd, "Convert brushes to meshes (new ids, same parents).";
        "mesh.create" Mutate mesh::Create => results::Created = mesh::create, "Create a polygon mesh from vertices and face loops (counter-clockwise from outside).";
        "mesh.to_brush" Mutate mesh::Convert => results::CreatedMapped = mesh::mesh_to_brush_cmd, "Convert closed convex meshes to brushes (new ids, same parents).";
        "mesh.delete_faces" Mutate mesh::DeleteFaces => results::FacesRemoved = mesh::delete_faces, "Remove faces from meshes; vertices no face uses are dropped.";
        "face.set_material" Mutate face::SetMaterial => results::FacesChanged = face::set_material, "Set the material of faces (b12/f3), or of every face of an object (b12).";
        "face.set_uv" Mutate face::SetUv => results::FacesChanged = face::set_uv, "Set face UV offset, scale and rotation; omitted fields are kept.";
        "face.translate" Mutate face::TranslateFaces => results::VerticesMoved = face::translate_faces, "Move the vertices of mesh or brush faces by a delta. A brush stays convex: vertices that meet fuse and a face that stops being planar splits.";
        "vertex.translate" Mutate face::TranslateVertices => results::VerticesMoved = face::translate_vertices, "Move mesh vertices (m3/v10), or brush corners given by world position in brush_vertices, by a delta. A brush stays convex; a corner that would end up inside it is InvalidGeometry.";
        "room.create" Mutate build::RoomCreate => results::RoomCreated = build::room_create, "Create a hollow room: floor, ceiling and four wall brushes around the interior min..max. result.roles maps floor, ceiling, wall_north/south/east/west to ids.";
        "room.cut_opening" Mutate build::CutOpening => results::CreatedMapped = build::cut_opening, "Cut a box-shaped opening (doorway, window, corridor mouth) through every brush it crosses. The cutter never enters the document; result.map lists target -> resulting ids.";
        "stairs.create" Mutate build::StairsCreate => results::StairsCreated = build::stairs_create, "Create stairs as non-overlapping step brushes, each solid from the floor up. result.ids run bottom to top; result.top is the landing point.";
        "csg.subtract" Mutate csg::Subtract => results::CreatedMapped = csg::subtract, "Subtract cutter brushes or meshes from targets. result.map lists each target -> resulting ids; consume_cutters deletes the cutters too (destructive).";
        "transform.translate" Mutate transform::Translate => results::Transformed = transform::translate, "Move objects by a delta. Groups and brush entities move their contents.";
        "transform.rotate" Mutate transform::Rotate => results::Transformed = transform::rotate, "Rotate objects about an axis (right-hand rule, degrees) around a pivot: center, origin or a point.";
        "transform.scale" Mutate transform::Scale => results::Transformed = transform::scale, "Scale objects by a factor or per-axis factors around a pivot; negative factors mirror.";
        "transform.flip" Mutate transform::Flip => results::Transformed = transform::flip, "Mirror objects across the plane through the pivot whose normal is the axis.";
        "object.delete" Destructive object::Delete => results::Deleted = object::delete, "Delete objects and everything they own. Undoable.";
        "object.duplicate" Mutate object::Duplicate => results::Duplicated = object::duplicate, "Deep-copy objects (with group and entity contents), optionally offset. result.map lists old -> new ids.";
        "object.array" Mutate object::Array => results::Arrayed = object::array, "Make count extra deep copies at multiples of an offset, optionally with a mirrored copy of the whole row, as one step. result.map lists old -> new ids.";
        "object.rename" Mutate object::Rename => results::IdOnly = object::rename, "Rename a group, layer or camera.";
        "object.set_hidden" Mutate visibility::SetHidden => results::Updated = visibility::set_hidden, "Hide or show objects with everything they contain. Hidden objects are not drawn, picked or raycast, and drop out of the selection.";
        "object.set_locked" Mutate visibility::SetLocked => results::Updated = visibility::set_locked, "Lock or unlock objects with everything they contain. An op that would edit, delete or add to a locked object fails with Locked.";
        "object.isolate" Mutate visibility::Isolate => results::Updated = visibility::isolate, "Hide everything except these objects, what they contain and what contains them.";
        "object.show_all" Mutate NoArgs => results::Updated = visibility::show_all, "Show every hidden object.";
        "object.unlock_all" Mutate NoArgs => results::Updated = visibility::unlock_all, "Unlock every locked object.";
        "object.set_layer" Mutate object::SetLayer => results::Moved = object::set_layer, "Move objects, with their contents, to another layer.";
        "entity.create" Mutate entity::Create => results::Created = entity::create, "Create a point entity with classname, origin, angles and properties.";
        "entity.create_brush_entity" Mutate entity::CreateBrushEntity => results::Created = entity::create_brush_entity, "Make a brush entity that owns the given brushes and meshes.";
        "entity.release_objects" Mutate entity::Release => results::Released = entity::release_objects, "Return brushes and meshes to the world; an emptied brush entity is deleted.";
        "entity.set_properties" Mutate entity::SetProperties => results::Updated = entity::set_properties, "Set and remove key/value properties on entities.";
        "entity.set_classname" Mutate entity::SetClassname => results::Updated = entity::set_classname, "Change the classname of entities.";
        "entity.set_transform" Mutate entity::SetTransform => results::IdOnly = entity::set_transform, "Set a point entity's origin and/or angles (pitch, yaw, roll) absolutely.";
        "layer.create" Mutate layer::Create => results::Created = layer::create, "Create a layer.";
        "layer.set_omit_from_export" Mutate layer::SetOmitFromExport => results::IdOnly = layer::set_omit_from_export, "Mark a layer to be left out of exports; nothing exports yet, so this is only stored.";
        "layer.delete" Destructive layer::Delete => results::LayerDeleted = layer::delete, "Delete a layer, moving its contents to move_to if it has any.";
        "group.create" Mutate group::Create => results::Created = group::create, "Group objects that share a layer and parent group.";
        "group.ungroup" Mutate group::Ungroup => results::Ungrouped = group::ungroup, "Dissolve groups; their contents go to the group's parent.";
        "group.move" Mutate group::Move => results::Moved = group::move_objects, "Move objects into a group, or out of any group when group is null.";
        "scene.summary" Read NoArgs => results::Summary = scene::summary, "Compact overview: counts by kind, bounds, layers (with their hidden, locked and omit_from_export flags), entity classnames, selection size and how many objects are hidden or locked.";
        "scene.list" Read scene::List => results::ObjectPage = scene::list, "Filter and page through objects by kind, layer, group, entity, classname, name or region.";
        "scene.get" Read scene::Get => results::Objects = scene::get, "Details of objects by id, at summary, full or geometry level.";
        "scene.bounds" Read scene::Bounds => results::BoundsResult = scene::bounds, "Axis-aligned bounds of objects, each and combined.";
        "scene.raycast" Read scene::Raycast => results::Raycast = scene::raycast, "First brush, mesh or point entity hit by a ray: id, face, point, normal, distance. Hidden objects are skipped unless include_hidden.";
        "scene.query_box" Read scene::QueryBox => results::IdCount = scene::query_box, "Objects whose bounds intersect or lie inside a box. Hidden objects are skipped unless include_hidden.";
        "scene.overlaps" Read scene::Overlaps => results::Overlaps = scene::overlaps, "Pairs of overlapping brushes and meshes (exact for brush pairs, bounds-only otherwise).";
        "check.validate" Read check::Validate => results::Validation = check::validate_cmd, "Run the issue checker and return structured issues.";
        "view.camera.list" Read NoArgs => results::CameraList = view::list, "List document cameras and the session viewport cameras.";
        "view.camera.get" Read view::Get => results::CameraOne = view::get, "Resolve a camera by id, name or viewport.* name.";
        "view.camera.create" Mutate view::CreateCameraArgs => results::Created = view::create, "Create a named perspective or orthographic camera.";
        "view.camera.set" Mutate view::SetCameraArgs => results::IdOnly = view::set, "Edit a document camera.";
        "view.camera.delete" Destructive view::Delete => results::Deleted = view::delete, "Delete document cameras.";
        "view.frame" Mutate view::Frame => results::Framed = view::frame, "Move a document camera so the given objects fill the image.";
        "batch" Mutate batch::Batch => results::BatchResults = batch::run, "Run commands as one all-or-nothing transaction. Items use dotted or underscored op names; name an item with `as` and reference results as $name, $N, $name.ids[k], $name/v2 or $name/f1.";
    }
    session {
        "history.begin" Mutate session_args::Begin => results::Empty, "Open a transaction; edits group into one undo step until commit or rollback.";
        "history.commit" Mutate NoArgs => results::Empty, "Commit the open transaction as one undo step.";
        "history.rollback" Mutate NoArgs => results::Empty, "Discard the open transaction.";
        "history.undo" Destructive NoArgs => results::Stepped, "Undo the last committed step.";
        "history.redo" Mutate NoArgs => results::Stepped, "Redo the last undone step.";
        "history.list" Read session_args::List => results::HistoryList, "List recent history entries with client attribution.";
        "select.get" Read NoArgs => results::IdCount, "The current editor selection (shared with the GUI).";
        "select.set" Mutate session_args::SelectIds => results::IdCount, "Replace the selection with these brushes, meshes, entities or groups.";
        "select.add" Mutate session_args::SelectIds => results::IdCount, "Add objects to the selection.";
        "select.remove" Mutate session_args::SelectIds => results::IdCount, "Remove objects from the selection.";
        "select.clear" Mutate NoArgs => results::IdCount, "Clear the selection.";
        "select.all" Mutate session_args::SelectKind => results::IdCount, "Select every selectable object, optionally of one kind. Hidden and locked objects are skipped.";
        "select.invert" Mutate session_args::SelectKind => results::IdCount, "Select what is not selected (optionally within one kind) and deselect the rest.";
        "file.info" Read NoArgs => results::FileInfoResult, "Path (null until the document has a file), name, dirty flag and revision of the open document.";
        "file.save" Mutate session_args::Save => results::Saved, "Write the document to its file, or to `path` (which becomes the document path; creates parent dirs). Fails with FileExists for another existing file unless overwrite is true.";
        "file.new" Destructive session_args::New => results::Empty, "Replace the document with an empty one, optionally bound to a new `path` (nothing is written until file.save). Fails with UnsavedChanges unless discard is true.";
        "file.open" Destructive session_args::Open => results::Empty, "Load a scene file, replacing the document. Fails with UnsavedChanges unless discard is true.";
    }
    client {
        "view.screenshot" Read screenshot::ScreenshotArgs => results::ScreenshotResult, "Render a PNG from a camera with optional overlays (grid, axes, labels, bounds) and highlighted objects.";
    }
    server {
        "session.list" Read NoArgs => results::SessionList, "List the server's open sessions (id, path, name, dirty, revision, connected clients) and which one is focused.";
        "session.attach" Read session_args::Attach => results::SessionAttached, "Attach this connection to a session by `id` or file `path` (loading it if no session has it; `create` makes a missing file). With neither, attach to the focused session.";
        "session.info" Read NoArgs => results::SessionRow, "The session this connection is attached to, with the connected clients.";
    }
}

/// Runs a document op and refuses it with `Locked` if it edited something locked. A batch checks each item, so it can unlock, edit and relock.
pub fn dispatch(ctx: &mut Ctx, op: &str, args: Value) -> CmdResult {
    let op = OpInfo::find(op).map_or(op, |o| o.name);
    let start = ctx.journal_len();
    let outcome = dispatch_op(ctx, op, args)?;
    if op != "batch" {
        locks::enforce(ctx, start)?;
    }
    Ok(outcome)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome<R = Value> {
    pub result: R,
    pub warnings: Vec<String>,
}

impl<R> Outcome<R> {
    pub fn new(result: R) -> Self {
        Self {
            result,
            warnings: Vec::new(),
        }
    }

    pub fn with_warnings(result: R, warnings: Vec<String>) -> Self {
        Self { result, warnings }
    }
}

impl<R: serde::Serialize> Outcome<R> {
    pub fn erase(self) -> CmdResult {
        let result = serde_json::to_value(self.result).map_err(|e| CommandError::Io {
            message: format!("result does not serialize: {e}"),
        })?;
        Ok(Outcome {
            result,
            warnings: self.warnings,
        })
    }
}

impl Outcome<results::Created> {
    pub fn created(ids: Vec<ObjectId>, warnings: Vec<String>) -> Self {
        Self::with_warnings(results::Created::new(ids), warnings)
    }
}

impl Outcome<results::CreatedMapped> {
    pub fn created_mapped(
        ids: Vec<ObjectId>,
        warnings: Vec<String>,
        map: Vec<(ObjectId, Vec<ObjectId>)>,
    ) -> Self {
        let id = ids.first().copied();
        Self::with_warnings(
            results::CreatedMapped {
                ids,
                id,
                map: map.into_iter().collect(),
            },
            warnings,
        )
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
        "session.attach" if flag("create") => info.safety.max(Safety::Mutate),
        _ => info.safety,
    })
}
