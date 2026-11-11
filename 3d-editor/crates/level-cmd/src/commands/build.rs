use glam::DVec3;
use level_core::{BrushId, BrushObject, Extra, Group, GroupId, LayerId, Object, ObjectId};
use level_geom::{Aabb, normalize, shapes};
use schemars::JsonSchema;
use serde::Deserialize;

use super::brush::{dominant_material, geometry_error, set_material};
use super::csg::subtract_brushes;
use super::results;
use super::util::{brush_of, check_finite, resolve_layer};
use super::{CmdResult, Outcome};
use crate::bounds::object_bounds;
use crate::ctx::Ctx;
use crate::error::CommandError;

struct Placement {
    layer: LayerId,
    group: Option<GroupId>,
    created_group: Option<GroupId>,
}

fn place(
    ctx: &mut Ctx,
    layer: Option<LayerId>,
    group: Option<GroupId>,
    name: Option<String>,
) -> Result<Placement, CommandError> {
    let (layer, parent) = match group {
        Some(g) => {
            let found = ctx
                .doc()
                .groups
                .get(&g)
                .ok_or_else(|| CommandError::not_found(g))?;
            if layer.is_some_and(|l| l != found.layer) {
                return Err(CommandError::invalid_arg(
                    "layer",
                    format!("{g} is on layer {}", found.layer),
                ));
            }
            (found.layer, Some(g))
        }
        None => (resolve_layer(ctx.doc(), layer)?, None),
    };
    let Some(name) = name else {
        return Ok(Placement {
            layer,
            group: parent,
            created_group: None,
        });
    };
    if name.trim().is_empty() {
        return Err(CommandError::invalid_arg("name", "name must not be empty"));
    }
    let id = GroupId(ctx.alloc_id());
    ctx.insert(Object::Group(Group {
        id,
        name,
        layer,
        parent,
        extra: Extra::new(),
    }));
    Ok(Placement {
        layer,
        group: Some(id),
        created_group: Some(id),
    })
}

fn add_box(
    ctx: &mut Ctx,
    at: &Placement,
    min: DVec3,
    max: DVec3,
    material: &str,
    what: &str,
) -> Result<ObjectId, CommandError> {
    let mut brush = shapes::cuboid(min, max);
    set_material(&mut brush, material);
    let brush = normalize(&brush).map_err(|e| geometry_error(what, e))?;
    let id = BrushId(ctx.alloc_id());
    ctx.insert(Object::Brush(BrushObject {
        id,
        layer: at.layer,
        group: at.group,
        entity: None,
        brush,
        extra: Extra::new(),
    }));
    Ok(id.into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum RoomSide {
    Floor,
    Ceiling,
    North,
    South,
    East,
    West,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RoomCreate {
    /// Interior minimum corner: the walkable space is min..max.
    #[schemars(with = "[f64; 3]")]
    min: DVec3,
    #[schemars(with = "[f64; 3]")]
    max: DVec3,
    /// Thickness of the floor, ceiling and walls, built outside the interior.
    wall: f64,
    #[serde(default)]
    material: String,
    floor_material: Option<String>,
    ceiling_material: Option<String>,
    wall_material: Option<String>,
    layer: Option<LayerId>,
    /// An existing group to put the room in.
    group: Option<GroupId>,
    /// Wrap the room in a new group with this name (inside `group` when both are given).
    name: Option<String>,
    /// Sides to leave out, e.g. ["east", "west"] for a corridor.
    #[serde(default)]
    open_sides: Vec<RoomSide>,
}

pub fn room_create(ctx: &mut Ctx, args: RoomCreate) -> CmdResult<results::RoomCreated> {
    check_finite("min", args.min)?;
    check_finite("max", args.max)?;
    if !args.min.cmplt(args.max).all() {
        return Err(CommandError::invalid_arg(
            "max",
            "max must be greater than min on every axis",
        ));
    }
    if !(args.wall.is_finite() && args.wall > 0.0) {
        return Err(CommandError::invalid_arg("wall", "wall must be positive"));
    }
    let t = DVec3::splat(args.wall);
    let v = DVec3::new;
    check_finite("wall", args.min - t)?;
    check_finite("wall", args.max + t)?;
    let at = place(ctx, args.layer, args.group, args.name.clone())?;

    let (min, max) = (args.min, args.max);
    let open = |side| args.open_sides.contains(&side);
    let pick = |is_open: bool, inner: f64, outer: f64| if is_open { inner } else { outer };
    let lo = v(
        pick(open(RoomSide::West), min.x, min.x - t.x),
        pick(open(RoomSide::South), min.y, min.y - t.y),
        min.z - t.z,
    );
    let hi = v(
        pick(open(RoomSide::East), max.x, max.x + t.x),
        pick(open(RoomSide::North), max.y, max.y + t.y),
        max.z + t.z,
    );
    let floor_m = args.floor_material.as_deref().unwrap_or(&args.material);
    let ceiling_m = args.ceiling_material.as_deref().unwrap_or(&args.material);
    let wall_m = args.wall_material.as_deref().unwrap_or(&args.material);
    let parts: [(&str, RoomSide, DVec3, DVec3, &str); 6] = [
        ("floor", RoomSide::Floor, lo, v(hi.x, hi.y, min.z), floor_m),
        (
            "ceiling",
            RoomSide::Ceiling,
            v(lo.x, lo.y, max.z),
            hi,
            ceiling_m,
        ),
        (
            "wall_north",
            RoomSide::North,
            v(lo.x, max.y, min.z),
            v(hi.x, max.y + t.y, max.z),
            wall_m,
        ),
        (
            "wall_south",
            RoomSide::South,
            v(lo.x, min.y - t.y, min.z),
            v(hi.x, min.y, max.z),
            wall_m,
        ),
        (
            "wall_east",
            RoomSide::East,
            v(max.x, min.y, min.z),
            v(max.x + t.x, max.y, max.z),
            wall_m,
        ),
        (
            "wall_west",
            RoomSide::West,
            v(min.x - t.x, min.y, min.z),
            v(min.x, max.y, max.z),
            wall_m,
        ),
    ];
    let mut ids = Vec::new();
    let mut roles = std::collections::BTreeMap::new();
    for (role, side, from, to, material) in parts {
        if args.open_sides.contains(&side) {
            continue;
        }
        let id = add_box(ctx, &at, from, to, material, role)?;
        roles.insert(role.to_string(), id);
        ids.push(id);
    }
    if ids.is_empty() {
        return Err(CommandError::invalid_arg(
            "open_sides",
            "every side is open, so there is nothing to build",
        ));
    }
    Ok(Outcome::new(results::RoomCreated {
        id: ids.first().copied(),
        ids,
        group: at.created_group,
        roles,
    }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CutOpening {
    #[schemars(with = "[f64; 3]")]
    min: DVec3,
    #[schemars(with = "[f64; 3]")]
    max: DVec3,
    /// Brushes to cut; default: every world brush the box crosses.
    targets: Option<Vec<ObjectId>>,
    /// Material of the new faces inside the opening; default: each target's most common material.
    material: Option<String>,
}

fn crosses(a: &Aabb, b: &Aabb) -> bool {
    a.min.cmplt(b.max).all() && b.min.cmplt(a.max).all()
}

pub fn cut_opening(ctx: &mut Ctx, args: CutOpening) -> CmdResult<results::CreatedMapped> {
    check_finite("min", args.min)?;
    check_finite("max", args.max)?;
    if !args.min.cmplt(args.max).all() {
        return Err(CommandError::invalid_arg(
            "max",
            "max must be greater than min on every axis",
        ));
    }
    let region = Aabb {
        min: args.min,
        max: args.max,
    };
    let targets: Vec<ObjectId> = match args.targets {
        Some(ids) => {
            for &id in &ids {
                brush_of(ctx.doc(), id).map_err(|e| match e {
                    CommandError::WrongKind { target, .. } => {
                        CommandError::invalid_arg("targets", format!("{target} is not a brush"))
                            .with_hint("room.cut_opening cuts brushes; use csg.subtract for meshes")
                    }
                    other => other,
                })?;
            }
            ids
        }
        None => ctx
            .doc()
            .brushes
            .values()
            .filter(|b| b.entity.is_none())
            .filter(|b| {
                object_bounds(&Object::Brush((*b).clone())).is_some_and(|bb| crosses(&bb, &region))
            })
            .map(|b| b.id.into())
            .collect(),
    };
    if targets.is_empty() {
        return Err(
            CommandError::invalid_arg("min", "the opening box does not cross any brush")
                .with_hint("check the box against scene.query_box, or name targets explicitly"),
        );
    }

    let mut created = Vec::new();
    let mut map = Vec::new();
    for id in targets {
        let target = brush_of(ctx.doc(), id)?.clone();
        let material = args
            .material
            .clone()
            .unwrap_or_else(|| dominant_material(&target.brush));
        let mut cutter = shapes::cuboid(args.min, args.max);
        set_material(&mut cutter, &material);
        let cutter = normalize(&cutter).map_err(|e| geometry_error("opening", e))?;
        let result = subtract_brushes(ctx, &target, &[cutter])?;
        created.extend(result.iter().copied().filter(|r| *r != id));
        map.push((id, result));
    }
    Ok(Outcome::created_mapped(created, vec![], map))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
pub enum StairDirection {
    #[serde(rename = "+x")]
    PosX,
    #[serde(rename = "-x")]
    NegX,
    #[serde(rename = "+y")]
    PosY,
    #[serde(rename = "-y")]
    NegY,
}

impl StairDirection {
    pub(super) fn vector(self) -> DVec3 {
        match self {
            Self::PosX => DVec3::X,
            Self::NegX => DVec3::NEG_X,
            Self::PosY => DVec3::Y,
            Self::NegY => DVec3::NEG_Y,
        }
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StairsCreate {
    /// Floor point at the centre of the first step's low edge.
    #[schemars(with = "[f64; 3]")]
    origin: DVec3,
    /// The way the stairs climb.
    direction: StairDirection,
    steps: u32,
    /// Height of each step.
    rise: f64,
    /// Depth of each step along `direction`.
    run: f64,
    /// Width across the stairs, centred on `origin`.
    width: f64,
    #[serde(default)]
    material: String,
    layer: Option<LayerId>,
    group: Option<GroupId>,
    name: Option<String>,
}

pub fn stairs_create(ctx: &mut Ctx, args: StairsCreate) -> CmdResult<results::StairsCreated> {
    check_finite("origin", args.origin)?;
    if !(1..=256).contains(&args.steps) {
        return Err(CommandError::invalid_arg(
            "steps",
            "steps must be between 1 and 256",
        ));
    }
    for (field, value) in [
        ("rise", args.rise),
        ("run", args.run),
        ("width", args.width),
    ] {
        if !(value.is_finite() && value > 0.0) {
            return Err(CommandError::invalid_arg(
                field,
                format!("{field} must be positive"),
            ));
        }
    }
    let forward = args.direction.vector();
    let side = DVec3::Z.cross(forward);
    let top = args.origin
        + forward * (f64::from(args.steps) * args.run)
        + DVec3::Z * (f64::from(args.steps) * args.rise);
    check_finite("steps", top)?;
    let at = place(ctx, args.layer, args.group, args.name)?;

    let mut ids = Vec::new();
    for i in 0..args.steps {
        let (a0, a1) = (f64::from(i) * args.run, f64::from(i + 1) * args.run);
        let half = side * (args.width / 2.0);
        let p0 = args.origin + forward * a0 - half;
        let p1 = args.origin + forward * a1 + half + DVec3::Z * (f64::from(i + 1) * args.rise);
        let (min, max) = (p0.min(p1), p0.max(p1));
        let min = DVec3::new(min.x, min.y, args.origin.z);
        ids.push(add_box(ctx, &at, min, max, &args.material, "step")?);
    }
    Ok(Outcome::new(results::StairsCreated {
        id: ids.first().copied(),
        ids,
        group: at.created_group,
        top,
    }))
}
