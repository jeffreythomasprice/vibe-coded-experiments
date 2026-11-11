use std::collections::BTreeSet;

use editor_core::{Camera, CameraId, Document, Extra, Object, ObjectId, OrthoView, Projection};
use editor_geom::Aabb;
use editor_geom::camera::{
    DEFAULT_FOV, MAX_IMAGE_DIM, MAX_PITCH, REFERENCE_HEIGHT, REFERENCE_WIDTH, VIEWPORTS,
    frame_camera, look_at, viewport_camera,
};
use glam::DVec3;
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::results;
use super::util::check_finite;
use super::{CmdResult, NoArgs, Outcome, parse_args};
use crate::bounds::{extent, scene_bounds};
use crate::ctx::Ctx;
use crate::error::CommandError;

#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CameraParams {
    #[schemars(with = "Option<[f64; 3]>")]
    pub position: Option<DVec3>,
    pub yaw: Option<f64>,
    pub pitch: Option<f64>,
    #[schemars(with = "Option<[f64; 3]>")]
    pub target: Option<DVec3>,
    pub fov: Option<f64>,
    pub ortho: Option<OrthoView>,
    pub zoom: Option<f64>,
}

fn check_number(field: &str, v: f64) -> Result<(), CommandError> {
    if v.is_finite() {
        Ok(())
    } else {
        Err(CommandError::invalid_arg(
            field,
            "values must be finite numbers",
        ))
    }
}

fn check_zoom(zoom: f64) -> Result<(), CommandError> {
    if zoom.is_finite() && zoom > 0.0 {
        Ok(())
    } else {
        Err(CommandError::invalid_arg(
            "zoom",
            "zoom (pixels per world unit) must be greater than 0",
        ))
    }
}

fn check_fov(fov: f64) -> Result<(), CommandError> {
    if fov.is_finite() && fov > 1.0 && fov < 179.0 {
        Ok(())
    } else {
        Err(CommandError::invalid_arg(
            "fov",
            "fov (vertical, degrees) must be between 1 and 179",
        ))
    }
}

fn ortho_viewport(view: OrthoView) -> &'static str {
    match view {
        OrthoView::Top => "viewport.top",
        OrthoView::Front => "viewport.front",
        OrthoView::Side => "viewport.side",
    }
}

impl CameraParams {
    pub fn apply(&self, mut cam: Camera) -> Result<Camera, CommandError> {
        if self.fov.is_some() && self.ortho.is_some() {
            return Err(CommandError::invalid_arg(
                "fov",
                "fov applies to perspective cameras, ortho to orthographic ones",
            )
            .with_hint("pass only one of fov and ortho"));
        }
        if self.target.is_some() && (self.yaw.is_some() || self.pitch.is_some()) {
            return Err(CommandError::invalid_arg(
                "target",
                "give either target or yaw/pitch, not both",
            ));
        }
        if let Some(p) = self.position {
            check_finite("position", p)?;
            cam.position = p;
        }
        if let Some(yaw) = self.yaw {
            check_number("yaw", yaw)?;
            cam.yaw = yaw;
        }
        if let Some(pitch) = self.pitch {
            if !pitch.is_finite() || pitch.abs() > MAX_PITCH {
                return Err(CommandError::invalid_arg(
                    "pitch",
                    format!("pitch must be within ±{MAX_PITCH} degrees"),
                ));
            }
            cam.pitch = pitch;
        }
        if let Some(target) = self.target {
            check_finite("target", target)?;
            let (yaw, pitch) = look_at(cam.position, target).ok_or_else(|| {
                CommandError::invalid_arg("target", "target is the camera position")
            })?;
            cam.yaw = yaw;
            cam.pitch = pitch;
        }
        if let Some(zoom) = self.zoom {
            check_zoom(zoom)?;
        }
        let current_zoom = match cam.projection {
            Projection::Orthographic { zoom, .. } => Some(zoom),
            Projection::Perspective { .. } => None,
        };
        cam.projection = match (self.ortho, self.fov) {
            (Some(view), _) => Projection::Orthographic {
                view,
                zoom: self.zoom.or(current_zoom).unwrap_or(1.0),
            },
            (None, Some(fov)) => {
                check_fov(fov)?;
                if self.zoom.is_some() {
                    return Err(CommandError::invalid_arg(
                        "zoom",
                        "zoom applies to orthographic cameras",
                    ));
                }
                Projection::Perspective { fov }
            }
            (None, None) => match cam.projection {
                Projection::Orthographic { view, zoom } => Projection::Orthographic {
                    view,
                    zoom: self.zoom.unwrap_or(zoom),
                },
                Projection::Perspective { fov } => {
                    if self.zoom.is_some() {
                        return Err(CommandError::invalid_arg(
                            "zoom",
                            "zoom applies to orthographic cameras",
                        )
                        .with_hint("pass `ortho` to make this an orthographic camera"));
                    }
                    Projection::Perspective { fov }
                }
            },
        };
        Ok(cam)
    }
}

/// Schema-only mirror of the `view.camera.create` arguments; `create` splits the raw value itself.
#[derive(JsonSchema)]
#[allow(dead_code)]
pub struct CreateCameraArgs {
    name: String,
    #[serde(flatten)]
    params: CameraParams,
}

/// Schema-only mirror of the `view.camera.set` arguments.
#[derive(JsonSchema)]
#[allow(dead_code)]
pub struct SetCameraArgs {
    camera: String,
    name: Option<String>,
    #[serde(flatten)]
    params: CameraParams,
}

fn split_args(
    args: Value,
    keys: &[&str],
) -> Result<(Map<String, Value>, CameraParams), CommandError> {
    let mut map = match args {
        Value::Null => Map::new(),
        Value::Object(m) => m,
        _ => return Err(CommandError::invalid_arg("args", "args must be an object")),
    };
    let mut head = Map::new();
    for key in keys {
        if let Some(v) = map.remove(*key) {
            head.insert((*key).to_string(), v);
        }
    }
    Ok((head, parse_args(Value::Object(map))?))
}

fn string_field(head: &Map<String, Value>, field: &str) -> Result<Option<String>, CommandError> {
    match head.get(field) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(CommandError::invalid_arg(
            field,
            format!("{field} must be a string"),
        )),
    }
}

pub(super) fn check_name(
    doc: &Document,
    name: &str,
    own: Option<CameraId>,
) -> Result<(), CommandError> {
    let bad = |message: &str| Err(CommandError::invalid_arg("name", message));
    if name.trim().is_empty() {
        return bad("name must not be empty");
    }
    if name.starts_with("viewport.") {
        return bad("names starting with `viewport.` are reserved for the session cameras");
    }
    if name.parse::<ObjectId>().is_ok() {
        return bad("a camera name must not look like an object id");
    }
    if doc
        .cameras
        .values()
        .any(|c| c.name == name && Some(c.id) != own)
    {
        return bad("a camera with this name already exists");
    }
    Ok(())
}

fn document_camera<'a>(doc: &'a Document, reference: &str) -> Result<&'a Camera, CommandError> {
    if let Ok(id) = reference.parse::<ObjectId>() {
        return match id {
            ObjectId::Camera(c) => doc
                .cameras
                .get(&c)
                .ok_or_else(|| CommandError::not_found(id)),
            _ if doc.contains(id) => Err(CommandError::wrong_kind(id, "camera")),
            _ => Err(CommandError::not_found(id)),
        };
    }
    if reference.starts_with("viewport.") {
        return Err(CommandError::invalid_arg(
            "camera",
            format!("{reference} is a session camera and cannot be edited"),
        )
        .with_hint("create a camera with view.camera.create and edit that"));
    }
    doc.cameras
        .values()
        .find(|c| c.name == reference)
        .ok_or_else(|| CommandError::not_found(reference))
}

/// Resolves an id, a camera name or a `viewport.*` name. Session cameras are framed for `width` x `height`.
pub fn resolve_camera(
    doc: &Document,
    reference: &str,
    width: u32,
    height: u32,
) -> Result<Camera, CommandError> {
    if reference.starts_with("viewport.") {
        return viewport_camera(reference, scene_bounds(doc).as_ref(), width, height).ok_or_else(
            || {
                CommandError::invalid_arg("camera", format!("unknown session camera `{reference}`"))
                    .with_hint(format!("session cameras: {}", VIEWPORTS.join(", ")))
            },
        );
    }
    document_camera(doc, reference).cloned()
}

pub fn camera_json(camera: &Camera, session: bool) -> Value {
    let mut v = serde_json::to_value(camera).expect("cameras serialize");
    if session {
        v["id"] = Value::Null;
        v["session"] = json!(true);
    }
    v.as_object_mut().map(|m| m.remove("extra"));
    v
}

pub fn camera_view(camera: &Camera, session: bool) -> results::CameraView {
    match camera_json(camera, session) {
        Value::Object(map) => results::CameraView(map),
        _ => unreachable!("cameras serialize to objects"),
    }
}

pub fn list(ctx: &Ctx, _: NoArgs) -> CmdResult<results::CameraList> {
    let doc = ctx.doc();
    let cameras = doc
        .cameras
        .values()
        .map(|c| camera_view(c, false))
        .collect();
    Ok(Outcome::new(results::CameraList {
        cameras,
        viewports: VIEWPORTS.iter().map(|v| v.to_string()).collect(),
    }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Get {
    camera: String,
}

pub fn get(ctx: &Ctx, args: Get) -> CmdResult<results::CameraOne> {
    let session = args.camera.starts_with("viewport.");
    let camera = resolve_camera(ctx.doc(), &args.camera, REFERENCE_WIDTH, REFERENCE_HEIGHT)?;
    Ok(Outcome::new(results::CameraOne {
        camera: camera_view(&camera, session),
    }))
}

/// A new camera from `create`-style params; ortho cameras without a position or zoom are framed on the scene.
pub fn build_camera(
    doc: &Document,
    name: String,
    id: CameraId,
    params: &CameraParams,
) -> Result<Camera, CommandError> {
    if params.ortho.is_none() && params.position.is_none() {
        return Err(
            CommandError::invalid_arg("position", "a perspective camera needs a position")
                .with_hint("or pass `ortho` (top, front, side) for an orthographic camera"),
        );
    }
    let base = Camera {
        id,
        name,
        position: DVec3::ZERO,
        yaw: 0.0,
        pitch: 0.0,
        projection: Projection::Perspective { fov: DEFAULT_FOV },
        extra: Extra::new(),
    };
    let mut camera = params.apply(base)?;
    if let Projection::Orthographic { view, .. } = camera.projection
        && (params.position.is_none() || params.zoom.is_none())
    {
        let bounds = scene_bounds(doc);
        let framed = viewport_camera(
            ortho_viewport(view),
            bounds.as_ref(),
            REFERENCE_WIDTH,
            REFERENCE_HEIGHT,
        )
        .expect("ortho viewport names are valid");
        if params.position.is_none() {
            camera.position = framed.position;
        }
        if params.zoom.is_none() {
            camera.projection = framed.projection;
        }
    }
    Ok(camera)
}

pub fn create(ctx: &mut Ctx, args: Value) -> CmdResult<results::Created> {
    let (head, params) = split_args(args, &["name"])?;
    let name = string_field(&head, "name")?
        .ok_or_else(|| CommandError::invalid_arg("name", "name is required"))?;
    check_name(ctx.doc(), &name, None)?;
    let id = CameraId(ctx.alloc_id());
    let camera = build_camera(ctx.doc(), name, id, &params)?;
    ctx.insert(Object::Camera(camera));
    Ok(Outcome::created(vec![id.into()], vec![]))
}

pub fn set(ctx: &mut Ctx, args: Value) -> CmdResult<results::IdOnly> {
    let (head, params) = split_args(args, &["camera", "name"])?;
    let reference = string_field(&head, "camera")?
        .ok_or_else(|| CommandError::invalid_arg("camera", "camera is required"))?;
    let current = document_camera(ctx.doc(), &reference)?.clone();
    let id = current.id;
    let mut camera = params.apply(current.clone())?;
    if let Some(name) = string_field(&head, "name")? {
        check_name(ctx.doc(), &name, Some(id))?;
        camera.name = name;
    }
    if camera != current {
        ctx.insert(Object::Camera(camera));
    }
    Ok(Outcome::new(results::IdOnly { id: id.into() }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    cameras: Vec<String>,
}

pub fn delete(ctx: &mut Ctx, args: Delete) -> CmdResult<results::Deleted> {
    if args.cameras.is_empty() {
        return Err(CommandError::invalid_arg(
            "cameras",
            "cameras must not be empty",
        ));
    }
    let mut ids = BTreeSet::new();
    for reference in &args.cameras {
        ids.insert(document_camera(ctx.doc(), reference)?.id);
    }
    for &id in &ids {
        ctx.remove(id.into());
    }
    Ok(Outcome::new(results::Deleted { deleted: ids.len() }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Frame {
    ids: Vec<ObjectId>,
    camera: String,
    width: Option<u32>,
    height: Option<u32>,
}

fn image_dim(field: &str, v: Option<u32>, default: u32) -> Result<u32, CommandError> {
    let v = v.unwrap_or(default);
    if (1..=MAX_IMAGE_DIM).contains(&v) {
        Ok(v)
    } else {
        Err(CommandError::invalid_arg(
            field,
            format!("{field} must be between 1 and {MAX_IMAGE_DIM}"),
        ))
    }
}

pub fn frame(ctx: &mut Ctx, args: Frame) -> CmdResult<results::Framed> {
    let width = image_dim("width", args.width, REFERENCE_WIDTH)?;
    let height = image_dim("height", args.height, REFERENCE_HEIGHT)?;
    let camera = document_camera(ctx.doc(), &args.camera)?.clone();
    if args.ids.is_empty() {
        return Err(CommandError::invalid_arg("ids", "ids must not be empty"));
    }
    let mut bounds: Option<Aabb> = None;
    for &id in &args.ids {
        if !ctx.doc().contains(id) {
            return Err(CommandError::not_found(id));
        }
        if let Some(b) = extent(ctx.doc(), id) {
            bounds = Some(bounds.map_or(b, |acc| acc.merge(b)));
        }
    }
    let bounds = bounds.ok_or_else(|| {
        CommandError::invalid_arg("ids", "none of the objects has any extent to frame")
    })?;
    let framed = frame_camera(&camera, &bounds, width, height);
    let result = results::Framed {
        id: framed.id.into(),
        camera: camera_view(&framed, false),
    };
    if framed != camera {
        ctx.insert(Object::Camera(framed));
    }
    Ok(Outcome::new(result))
}
