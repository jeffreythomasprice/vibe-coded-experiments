use editor_core::{Brush, BrushId, BrushObject, Extra, FaceAttrs, LayerId, Object, ObjectId};
use editor_geom::{BrushError, clip_brush, normalize, shapes};
use glam::DVec3;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results;
use super::util::{Parents, brush_of, check_finite, resolve_layer};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

pub fn geometry_error(target: impl ToString, e: BrushError) -> CommandError {
    CommandError::invalid_geometry(
        target,
        e.to_string(),
        "check the plane points and make sure the shape encloses a volume",
    )
}

pub(super) fn set_material(brush: &mut Brush, material: &str) {
    for f in &mut brush.faces {
        f.attrs = FaceAttrs::with_material(material);
    }
}

fn add_brush(ctx: &mut Ctx, brush: Brush, layer: LayerId) -> ObjectId {
    let id = BrushId(ctx.alloc_id());
    ctx.insert(Object::Brush(BrushObject {
        flags: Default::default(),
        id,
        layer,
        group: None,
        entity: None,
        brush,
        extra: Extra::new(),
    }));
    id.into()
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateBox {
    #[schemars(with = "[f64; 3]")]
    pub min: DVec3,
    #[schemars(with = "[f64; 3]")]
    pub max: DVec3,
    pub layer: Option<LayerId>,
    #[serde(default)]
    pub material: String,
}

pub fn create_box(ctx: &mut Ctx, args: CreateBox) -> CmdResult<results::Created> {
    check_finite("min", args.min)?;
    check_finite("max", args.max)?;
    if !args.min.cmplt(args.max).all() {
        return Err(CommandError::invalid_arg(
            "max",
            "max must be greater than min on every axis",
        ));
    }
    let layer = resolve_layer(ctx.doc(), args.layer)?;
    let mut brush = shapes::cuboid(args.min, args.max);
    set_material(&mut brush, &args.material);
    let brush = normalize(&brush).map_err(|e| geometry_error("box", e))?;
    let id = add_brush(ctx, brush, layer);
    Ok(Outcome::created(vec![id], vec![]))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateWedge {
    #[schemars(with = "[f64; 3]")]
    min: DVec3,
    #[schemars(with = "[f64; 3]")]
    max: DVec3,
    /// The high end: the ramp rises toward this side of the box.
    direction: super::build::StairDirection,
    layer: Option<LayerId>,
    #[serde(default)]
    material: String,
}

pub fn create_wedge(ctx: &mut Ctx, args: CreateWedge) -> CmdResult<results::Created> {
    check_finite("min", args.min)?;
    check_finite("max", args.max)?;
    if !args.min.cmplt(args.max).all() {
        return Err(CommandError::invalid_arg(
            "max",
            "max must be greater than min on every axis",
        ));
    }
    let layer = resolve_layer(ctx.doc(), args.layer)?;
    let mut brush = shapes::wedge(args.min, args.max, args.direction.vector());
    set_material(&mut brush, &args.material);
    let brush = normalize(&brush).map_err(|e| geometry_error("wedge", e))?;
    let id = add_brush(ctx, brush, layer);
    Ok(Outcome::created(vec![id], vec![]))
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
    #[default]
    Z,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreatePrism {
    #[schemars(with = "[f64; 3]")]
    center: DVec3,
    radius: f64,
    sides: usize,
    height: f64,
    #[serde(default)]
    axis: Axis,
    layer: Option<LayerId>,
    #[serde(default)]
    material: String,
}

pub fn create_prism(ctx: &mut Ctx, args: CreatePrism) -> CmdResult<results::Created> {
    check_finite("center", args.center)?;
    if !(args.radius.is_finite() && args.radius > 0.0) {
        return Err(CommandError::invalid_arg(
            "radius",
            "radius must be positive",
        ));
    }
    if !(args.height.is_finite() && args.height > 0.0) {
        return Err(CommandError::invalid_arg(
            "height",
            "height must be positive",
        ));
    }
    if !(3..=256).contains(&args.sides) {
        return Err(CommandError::invalid_arg(
            "sides",
            "sides must be between 3 and 256",
        ));
    }
    let layer = resolve_layer(ctx.doc(), args.layer)?;

    let c = args.center;
    let (local_center, to_world): (DVec3, fn(DVec3) -> DVec3) = match args.axis {
        Axis::Z => (c, |p| p),
        Axis::X => (DVec3::new(c.y, c.z, c.x), |p| DVec3::new(p.z, p.x, p.y)),
        Axis::Y => (DVec3::new(c.z, c.x, c.y), |p| DVec3::new(p.y, p.z, p.x)),
    };
    let mut brush = shapes::prism(local_center, args.radius, args.height, args.sides);
    for f in &mut brush.faces {
        f.points = f.points.map(to_world);
    }
    set_material(&mut brush, &args.material);
    let brush = normalize(&brush).map_err(|e| geometry_error("prism", e))?;
    let id = add_brush(ctx, brush, layer);
    Ok(Outcome::created(vec![id], vec![]))
}

pub(super) fn dominant_material(brush: &Brush) -> String {
    let mut counts: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for f in &brush.faces {
        *counts.entry(f.attrs.material.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, n)| n)
        .map_or_else(String::new, |(m, _)| m.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Keep {
    Front,
    Back,
    Both,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    ids: Vec<ObjectId>,
    #[schemars(with = "[[f64; 3]; 3]")]
    plane: [DVec3; 3],
    keep: Keep,
    #[serde(default)]
    material: String,
}

pub fn clip(ctx: &mut Ctx, args: Clip) -> CmdResult<results::CreatedMapped> {
    if args.ids.is_empty() {
        return Err(CommandError::invalid_arg("ids", "ids must not be empty"));
    }
    for p in args.plane {
        check_finite("plane", p)?;
    }
    let mut created = Vec::new();
    let mut warnings = Vec::new();
    let mut map = Vec::new();

    for &id in &args.ids {
        let original = brush_of(ctx.doc(), id)?.clone();
        let material = if args.material.is_empty() {
            dominant_material(&original.brush)
        } else {
            args.material.clone()
        };
        let result = clip_brush(
            &original.brush,
            args.plane,
            FaceAttrs::with_material(&material),
        )
        .map_err(|e| geometry_error(id, e))?;

        if result.front.is_none() || result.back.is_none() {
            warnings.push(format!("the clip plane does not cut through {id}"));
        }

        let replace = |ctx: &mut Ctx, brush: Brush| {
            ctx.insert(Object::Brush(BrushObject {
                brush,
                ..original.clone()
            }));
        };

        match args.keep {
            Keep::Front | Keep::Back => {
                let kept = if args.keep == Keep::Front {
                    result.front
                } else {
                    result.back
                };
                let side = if args.keep == Keep::Front {
                    "front"
                } else {
                    "back"
                };
                let brush = kept.ok_or_else(|| {
                    CommandError::invalid_geometry(
                        id,
                        format!("the {side} side of the clip plane is empty"),
                        "flip the plane point order to keep the other side, or use keep: both",
                    )
                })?;
                replace(ctx, brush);
                map.push((id, vec![id]));
            }
            Keep::Both => {
                map.push((id, vec![id]));
                if let (Some(front), Some(back)) = (result.front, result.back) {
                    replace(ctx, back);
                    let parents = Parents::of_brush(&original);
                    let new_id = BrushId(ctx.alloc_id());
                    ctx.insert(Object::Brush(BrushObject {
                        flags: parents.flags,
                        id: new_id,
                        layer: parents.layer,
                        group: parents.group,
                        entity: parents.entity,
                        brush: front,
                        extra: parents.extra,
                    }));
                    created.push(new_id.into());
                    if let Some((_, to)) = map.last_mut() {
                        to.push(new_id.into());
                    }
                }
            }
        }
    }
    Ok(Outcome::created_mapped(created, warnings, map))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlaneFace {
    /// Three points; the outward normal is (c - a) x (b - a).
    #[schemars(with = "[[f64; 3]; 3]")]
    plane: [DVec3; 3],
    #[serde(default)]
    material: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateFromPlanes {
    faces: Vec<PlaneFace>,
    layer: Option<LayerId>,
}

pub fn create_from_planes(ctx: &mut Ctx, args: CreateFromPlanes) -> CmdResult<results::Created> {
    let layer = resolve_layer(ctx.doc(), args.layer)?;
    let mut brush = Brush::default();
    for (i, f) in args.faces.iter().enumerate() {
        for p in f.plane {
            check_finite("faces", p)?;
        }
        brush.faces.push(editor_core::BrushFace {
            id: editor_core::FaceId(i as u32 + 1),
            points: f.plane,
            attrs: FaceAttrs::with_material(&f.material),
        });
    }
    let given = brush.faces.len();
    let brush = normalize(&brush).map_err(|e| geometry_error("brush", e))?;
    let mut warnings = Vec::new();
    if brush.faces.len() < given {
        warnings.push(format!(
            "{} redundant planes were dropped",
            given - brush.faces.len()
        ));
    }
    let id = add_brush(ctx, brush, layer);
    Ok(Outcome::created(vec![id], warnings))
}
