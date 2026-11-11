use std::collections::BTreeSet;

use editor_core::{Object, ObjectId};
use editor_geom::tolerance::WORLD_BOUND;
use editor_geom::transform::{transform_brush, transform_mesh};
use glam::{DAffine3, DMat3, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::brush::{Axis, geometry_error};
use super::results;
use super::util::{check_finite, descendants, non_empty};
use super::{CmdResult, Outcome};
use crate::bounds::object_bounds;
use crate::ctx::Ctx;
use crate::error::CommandError;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum AxisArg {
    Named(Axis),
    Vector(#[schemars(with = "[f64; 3]")] DVec3),
}

impl AxisArg {
    pub(super) fn unit(&self, field: &str) -> Result<DVec3, CommandError> {
        let v = match self {
            AxisArg::Named(Axis::X) => DVec3::X,
            AxisArg::Named(Axis::Y) => DVec3::Y,
            AxisArg::Named(Axis::Z) => DVec3::Z,
            AxisArg::Vector(v) => *v,
        };
        if !v.is_finite() {
            return Err(CommandError::invalid_arg(field, "axis must be finite"));
        }
        v.try_normalize()
            .ok_or_else(|| CommandError::invalid_arg(field, "axis must not be the zero vector"))
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PivotName {
    Center,
    Origin,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Pivot {
    Named(PivotName),
    Point(#[schemars(with = "[f64; 3]")] DVec3),
}

impl Default for Pivot {
    fn default() -> Self {
        Pivot::Named(PivotName::Center)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Factors {
    Uniform(f64),
    PerAxis(#[schemars(with = "[f64; 3]")] DVec3),
}

fn orientation(angles: DVec3) -> DMat3 {
    DMat3::from_rotation_z(angles.y.to_radians())
        * DMat3::from_rotation_y(-angles.x.to_radians())
        * DMat3::from_rotation_x(angles.z.to_radians())
}

fn angles_of(m: DMat3) -> DVec3 {
    let f = m.x_axis;
    let yaw = f.y.atan2(f.x);
    let pitch = f.z.clamp(-1.0, 1.0).asin();
    let base = DMat3::from_rotation_z(yaw) * DMat3::from_rotation_y(-pitch);
    let r = base.transpose() * m;
    let roll = r.y_axis.z.atan2(r.y_axis.y);
    DVec3::new(pitch, yaw, roll).map(|a| {
        let degrees = a.to_degrees();
        (degrees * 1e6).round() / 1e6 + 0.0
    })
}

fn is_rotation(m: &DMat3) -> bool {
    let tol = 1e-9;
    (m.determinant() - 1.0).abs() < tol
        && (m.x_axis.length() - 1.0).abs() < tol
        && (m.y_axis.length() - 1.0).abs() < tol
        && m.x_axis.dot(m.y_axis).abs() < tol
}

fn check_bounds(object: &Object) -> Result<(), CommandError> {
    if let Some(b) = object_bounds(object)
        && b.min.abs().max_element().max(b.max.abs().max_element()) > WORLD_BOUND
    {
        return Err(CommandError::invalid_geometry(
            object.id(),
            "the result leaves the world bounds",
            "use smaller factors or offsets",
        ));
    }
    Ok(())
}

/// The object with `m` applied. Groups, layers and cameras have no geometry and come back unchanged.
pub fn transform_object(object: &Object, m: &DAffine3) -> Result<Object, CommandError> {
    let out = match object {
        Object::Brush(b) => {
            let mut b = b.clone();
            b.brush = transform_brush(&b.brush, m).map_err(|e| geometry_error(b.id, e))?;
            Object::Brush(b)
        }
        Object::Mesh(mesh) => {
            let mut mesh = mesh.clone();
            mesh.mesh = transform_mesh(&mesh.mesh, m);
            Object::Mesh(mesh)
        }
        Object::Entity(e) => {
            let mut e = e.clone();
            if let Some(origin) = e.origin {
                e.origin = Some(m.transform_point3(origin));
                if is_rotation(&m.matrix3) {
                    e.angles = angles_of(m.matrix3 * orientation(e.angles));
                }
            }
            Object::Entity(e)
        }
        other => other.clone(),
    };
    check_bounds(&out)?;
    Ok(out)
}

fn gather(ctx: &Ctx, ids: &[ObjectId]) -> Result<Vec<Object>, CommandError> {
    non_empty("ids", ids)?;
    let mut set = BTreeSet::new();
    for &id in ids {
        super::util::world_object(ctx.doc(), id)?;
        descendants(ctx.doc(), id, &mut set);
    }
    let leaves: Vec<Object> = set
        .into_iter()
        .filter_map(|id| ctx.doc().get(id))
        .filter(|o| match o {
            Object::Brush(_) | Object::Mesh(_) => true,
            Object::Entity(e) => e.origin.is_some(),
            _ => false,
        })
        .collect();
    if leaves.is_empty() {
        return Err(CommandError::invalid_arg(
            "ids",
            "the objects contain nothing that can be transformed",
        ));
    }
    Ok(leaves)
}

fn pivot_point(pivot: Pivot, leaves: &[Object]) -> Result<DVec3, CommandError> {
    match pivot {
        Pivot::Named(PivotName::Origin) => Ok(DVec3::ZERO),
        Pivot::Named(PivotName::Center) => leaves
            .iter()
            .filter_map(object_bounds)
            .reduce(|a, b| a.merge(b))
            .map(|b| b.center())
            .ok_or_else(|| CommandError::invalid_arg("pivot", "nothing to take the centre of")),
        Pivot::Point(p) => {
            check_finite("pivot", p)?;
            Ok(p)
        }
    }
}

pub(super) fn reflection(n: DVec3) -> DMat3 {
    DMat3::IDENTITY - DMat3::from_cols(n * n.x, n * n.y, n * n.z) * 2.0
}

pub(super) fn about(pivot: DVec3, linear: DMat3) -> DAffine3 {
    DAffine3::from_translation(pivot)
        * DAffine3::from_mat3(linear)
        * DAffine3::from_translation(-pivot)
}

fn apply(ctx: &mut Ctx, leaves: &[Object], m: &DAffine3) -> CmdResult<results::Transformed> {
    let moved = leaves
        .iter()
        .map(|o| transform_object(o, m))
        .collect::<Result<Vec<_>, _>>()?;
    let count = moved.len();
    for object in moved {
        ctx.insert(object);
    }
    Ok(Outcome::new(results::Transformed { transformed: count }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Translate {
    pub ids: Vec<ObjectId>,
    #[schemars(with = "[f64; 3]")]
    pub delta: DVec3,
}

pub fn translate(ctx: &mut Ctx, args: Translate) -> CmdResult<results::Transformed> {
    check_finite("delta", args.delta)?;
    let leaves = gather(ctx, &args.ids)?;
    apply(ctx, &leaves, &DAffine3::from_translation(args.delta))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rotate {
    pub ids: Vec<ObjectId>,
    pub axis: AxisArg,
    pub degrees: f64,
    #[serde(default)]
    pub pivot: Pivot,
}

pub fn rotate(ctx: &mut Ctx, args: Rotate) -> CmdResult<results::Transformed> {
    if !args.degrees.is_finite() {
        return Err(CommandError::invalid_arg(
            "degrees",
            "degrees must be a finite number",
        ));
    }
    let axis = args.axis.unit("axis")?;
    let leaves = gather(ctx, &args.ids)?;
    let pivot = pivot_point(args.pivot, &leaves)?;
    let linear = DMat3::from_axis_angle(axis, args.degrees.to_radians());
    apply(ctx, &leaves, &about(pivot, linear))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scale {
    pub ids: Vec<ObjectId>,
    pub factors: Factors,
    #[serde(default)]
    pub pivot: Pivot,
}

pub fn scale(ctx: &mut Ctx, args: Scale) -> CmdResult<results::Transformed> {
    let factors = match args.factors {
        Factors::Uniform(f) => DVec3::splat(f),
        Factors::PerAxis(v) => v,
    };
    if !factors.is_finite() || factors.abs().min_element() < 1e-9 {
        return Err(
            CommandError::invalid_arg("factors", "factors must be finite and non-zero")
                .with_hint("negative factors mirror; use a small positive factor to shrink"),
        );
    }
    let leaves = gather(ctx, &args.ids)?;
    let pivot = pivot_point(args.pivot, &leaves)?;
    apply(ctx, &leaves, &about(pivot, DMat3::from_diagonal(factors)))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Flip {
    pub ids: Vec<ObjectId>,
    pub axis: AxisArg,
    #[serde(default)]
    pub pivot: Pivot,
}

pub fn flip(ctx: &mut Ctx, args: Flip) -> CmdResult<results::Transformed> {
    let n = args.axis.unit("axis")?;
    let leaves = gather(ctx, &args.ids)?;
    let pivot = pivot_point(args.pivot, &leaves)?;
    apply(ctx, &leaves, &about(pivot, reflection(n)))
}
