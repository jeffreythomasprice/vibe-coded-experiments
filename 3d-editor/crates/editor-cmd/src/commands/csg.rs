use editor_core::{Brush, BrushId, BrushObject, Mesh, MeshId, MeshObject, Object, ObjectId};
use editor_geom::mesh::{
    CsgError, intersect as mesh_intersect, subtract as mesh_subtract, union as mesh_union,
};
use editor_geom::{
    BrushError, brush_inset, brush_intersect, brush_merge, brush_subtract, brush_to_mesh,
};
use schemars::JsonSchema;
use serde::Deserialize;

use super::brush::geometry_error;
use super::results;
use super::util::{Parents, brush_of, brush_or_mesh};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    Brush,
    Mesh,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Subtract {
    targets: Vec<ObjectId>,
    cutters: Vec<ObjectId>,
    output: Option<Output>,
    /// Delete the cutters in the same step.
    #[serde(default)]
    consume_cutters: bool,
}

fn csg_error(e: CsgError, target: ObjectId, cutter: ObjectId) -> CommandError {
    match e {
        CsgError::FirstNotClosed => CommandError::invalid_geometry(
            target,
            e.to_string(),
            "run check.validate on the target and repair it first",
        ),
        CsgError::SecondNotClosed => CommandError::invalid_geometry(
            cutter,
            e.to_string(),
            "run check.validate on the cutter and repair it first",
        ),
        CsgError::InvalidResult => CommandError::invalid_geometry(
            target,
            e.to_string(),
            "nudge a cutter by one grid step and retry",
        ),
    }
}

fn as_mesh(object: &Object) -> Result<Mesh, CommandError> {
    match object {
        Object::Brush(b) => brush_to_mesh(&b.brush).map_err(|e| geometry_error(b.id, e)),
        Object::Mesh(m) => Ok(m.mesh.clone()),
        _ => unreachable!("operands are checked to be brushes or meshes"),
    }
}

pub fn subtract(ctx: &mut Ctx, args: Subtract) -> CmdResult<results::CreatedMapped> {
    if args.targets.is_empty() {
        return Err(CommandError::invalid_arg(
            "targets",
            "targets must not be empty",
        ));
    }
    if args.cutters.is_empty() {
        return Err(CommandError::invalid_arg(
            "cutters",
            "cutters must not be empty",
        ));
    }
    if let Some(shared) = args.targets.iter().find(|t| args.cutters.contains(t)) {
        return Err(CommandError::invalid_arg(
            "cutters",
            format!("{shared} is both a target and a cutter"),
        ));
    }

    let targets: Vec<Object> = args
        .targets
        .iter()
        .map(|&id| brush_or_mesh(ctx.doc(), id))
        .collect::<Result<_, _>>()?;
    let cutters: Vec<Object> = args
        .cutters
        .iter()
        .map(|&id| brush_or_mesh(ctx.doc(), id))
        .collect::<Result<_, _>>()?;

    let any_mesh = targets
        .iter()
        .chain(&cutters)
        .any(|o| matches!(o, Object::Mesh(_)));
    if any_mesh && args.output == Some(Output::Brush) {
        return Err(
            CommandError::invalid_arg("output", "a mesh operand forces mesh output")
                .with_hint("omit output or use \"mesh\""),
        );
    }
    let mesh_mode = any_mesh || args.output == Some(Output::Mesh);

    let mut created = Vec::new();
    let mut warnings = Vec::new();

    let mut map = Vec::new();
    for target in &targets {
        let result = if mesh_mode {
            subtract_mesh(ctx, target, &cutters, &mut warnings)?
        } else {
            subtract_brush(ctx, target, &cutters)?
        };
        created.extend(result.iter().copied().filter(|id| *id != target.id()));
        map.push((target.id(), result));
    }
    if args.consume_cutters {
        for cutter in &args.cutters {
            ctx.remove(*cutter);
        }
    }
    Ok(Outcome::created_mapped(created, warnings, map))
}

fn subtract_brush(
    ctx: &mut Ctx,
    target: &Object,
    cutters: &[Object],
) -> Result<Vec<ObjectId>, CommandError> {
    let Object::Brush(t) = target else {
        unreachable!("brush mode has only brush operands")
    };
    let cutters: Vec<Brush> = cutters
        .iter()
        .map(|c| match c {
            Object::Brush(c) => c.brush.clone(),
            _ => unreachable!(),
        })
        .collect();
    subtract_brushes(ctx, t, &cutters)
}

pub(super) fn subtract_brushes(
    ctx: &mut Ctx,
    t: &BrushObject,
    cutters: &[Brush],
) -> Result<Vec<ObjectId>, CommandError> {
    let mut fragments: Vec<Brush> = vec![t.brush.clone()];
    for cutter in cutters {
        let mut next = Vec::new();
        for frag in &fragments {
            next.extend(brush_subtract(frag, cutter).map_err(|e| geometry_error(t.id, e))?);
        }
        fragments = next;
    }

    if fragments.len() == 1 && fragments[0] == t.brush {
        return Ok(vec![t.id.into()]);
    }
    Ok(replace_with_fragments(ctx, t, fragments))
}

/// Zero fragments delete the brush, one keeps its id, several replace it with new brushes in the same parents.
fn replace_with_fragments(
    ctx: &mut Ctx,
    t: &BrushObject,
    mut fragments: Vec<Brush>,
) -> Vec<ObjectId> {
    let parents = Parents::of_brush(t);
    let mut ids: Vec<ObjectId> = Vec::new();
    match fragments.len() {
        0 => {
            ctx.remove(t.id.into());
        }
        1 => {
            let brush = fragments.pop().expect("one fragment");
            ctx.insert(Object::Brush(BrushObject { brush, ..t.clone() }));
            ids.push(t.id.into());
        }
        _ => {
            ctx.remove(t.id.into());
            for brush in fragments {
                let id = BrushId(ctx.alloc_id());
                ctx.insert(Object::Brush(BrushObject {
                    flags: parents.flags,
                    id,
                    layer: parents.layer,
                    group: parents.group,
                    entity: parents.entity,
                    brush,
                    extra: parents.extra.clone(),
                }));
                ids.push(id.into());
            }
        }
    }
    ids
}

fn subtract_mesh(
    ctx: &mut Ctx,
    target: &Object,
    cutters: &[Object],
    warnings: &mut Vec<String>,
) -> Result<Vec<ObjectId>, CommandError> {
    let target_id = target.id();
    let mut acc = as_mesh(target)?;
    for cutter in cutters {
        let cutter_mesh = as_mesh(cutter)?;
        acc =
            mesh_subtract(&acc, &cutter_mesh).map_err(|e| csg_error(e, target_id, cutter.id()))?;
    }

    if acc.faces.is_empty() {
        warnings.push(format!("{target_id} was removed completely by the cutters"));
        ctx.remove(target_id);
        return Ok(Vec::new());
    }

    match target {
        Object::Mesh(m) => {
            ctx.insert(Object::Mesh(MeshObject {
                mesh: acc,
                ..m.clone()
            }));
            Ok(vec![target_id])
        }
        Object::Brush(b) => {
            ctx.remove(b.id.into());
            let parents = Parents::of_brush(b);
            let id = MeshId(ctx.alloc_id());
            ctx.insert(Object::Mesh(MeshObject {
                flags: parents.flags,
                id,
                layer: parents.layer,
                group: parents.group,
                entity: parents.entity,
                mesh: acc,
                extra: parents.extra,
            }));
            Ok(vec![id.into()])
        }
        _ => unreachable!(),
    }
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Union {
    /// At least two brushes or meshes. The first keeps its id; the others are deleted.
    ids: Vec<ObjectId>,
    output: Option<Output>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Intersect {
    /// At least two brushes or meshes. The first keeps its id; the others are deleted.
    ids: Vec<ObjectId>,
    output: Option<Output>,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Hollow {
    ids: Vec<ObjectId>,
    /// Wall thickness, measured inward from every face.
    thickness: f64,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Combine {
    Union,
    Intersect,
}

fn check_distinct(field: &str, ids: &[ObjectId]) -> Result<(), CommandError> {
    for (i, id) in ids.iter().enumerate() {
        if ids[..i].contains(id) {
            return Err(CommandError::invalid_arg(
                field,
                format!("{id} is listed twice"),
            ));
        }
    }
    Ok(())
}

pub fn union(ctx: &mut Ctx, args: Union) -> CmdResult<results::CreatedMapped> {
    combine(ctx, &args.ids, args.output, Combine::Union)
}

pub fn intersect(ctx: &mut Ctx, args: Intersect) -> CmdResult<results::CreatedMapped> {
    combine(ctx, &args.ids, args.output, Combine::Intersect)
}

fn combine(
    ctx: &mut Ctx,
    ids: &[ObjectId],
    output: Option<Output>,
    kind: Combine,
) -> CmdResult<results::CreatedMapped> {
    if ids.len() < 2 {
        return Err(CommandError::invalid_arg(
            "ids",
            "give at least two brushes or meshes",
        ));
    }
    check_distinct("ids", ids)?;
    let operands: Vec<Object> = ids
        .iter()
        .map(|&id| brush_or_mesh(ctx.doc(), id))
        .collect::<Result<_, _>>()?;
    let any_mesh = operands.iter().any(|o| matches!(o, Object::Mesh(_)));
    if any_mesh && output == Some(Output::Brush) {
        return Err(
            CommandError::invalid_arg("output", "a mesh operand forces mesh output")
                .with_hint("omit output or use \"mesh\""),
        );
    }
    let first = &operands[0];
    let first_id = first.id();
    let empty = || {
        CommandError::invalid_geometry(
            first_id,
            "the objects do not overlap",
            "move them so they share volume, or use csg.union",
        )
    };

    if any_mesh || output == Some(Output::Mesh) {
        let mut acc = as_mesh(first)?;
        for other in &operands[1..] {
            let other_mesh = as_mesh(other)?;
            acc = match kind {
                Combine::Union => mesh_union(&acc, &other_mesh),
                Combine::Intersect => mesh_intersect(&acc, &other_mesh),
            }
            .map_err(|e| csg_error(e, first_id, other.id()))?;
        }
        if acc.faces.is_empty() {
            return Err(empty());
        }
        let result: ObjectId = match first {
            Object::Mesh(m) => {
                ctx.insert(Object::Mesh(MeshObject {
                    mesh: acc,
                    ..m.clone()
                }));
                m.id.into()
            }
            Object::Brush(b) => {
                ctx.remove(b.id.into());
                let parents = Parents::of_brush(b);
                let id = MeshId(ctx.alloc_id());
                ctx.insert(Object::Mesh(MeshObject {
                    flags: parents.flags,
                    id,
                    layer: parents.layer,
                    group: parents.group,
                    entity: parents.entity,
                    mesh: acc,
                    extra: parents.extra,
                }));
                id.into()
            }
            _ => unreachable!(),
        };
        return Ok(finish_combine(ctx, ids, result));
    }

    let brushes: Vec<&BrushObject> = operands
        .iter()
        .map(|o| match o {
            Object::Brush(b) => b,
            _ => unreachable!("brush mode has only brush operands"),
        })
        .collect();
    let merged = match kind {
        Combine::Union => {
            let all: Vec<Brush> = brushes.iter().map(|b| b.brush.clone()).collect();
            brush_merge(&all).map_err(|e| geometry_error(first_id, e))?
        }
        Combine::Intersect => {
            let mut acc = brushes[0].brush.clone();
            for other in &brushes[1..] {
                acc = brush_intersect(&acc, &other.brush)
                    .map_err(|e| geometry_error(first_id, e))?
                    .ok_or_else(empty)?;
            }
            acc
        }
    };
    ctx.insert(Object::Brush(BrushObject {
        brush: merged,
        ..brushes[0].clone()
    }));
    Ok(finish_combine(ctx, ids, first_id))
}

fn finish_combine(
    ctx: &mut Ctx,
    ids: &[ObjectId],
    result: ObjectId,
) -> Outcome<results::CreatedMapped> {
    for id in &ids[1..] {
        ctx.remove(*id);
    }
    let created = if result == ids[0] {
        Vec::new()
    } else {
        vec![result]
    };
    Outcome::created_mapped(
        created,
        Vec::new(),
        ids.iter().map(|&id| (id, vec![result])).collect(),
    )
}

pub fn hollow(ctx: &mut Ctx, args: Hollow) -> CmdResult<results::CreatedMapped> {
    if args.ids.is_empty() {
        return Err(CommandError::invalid_arg("ids", "ids must not be empty"));
    }
    if !args.thickness.is_finite() || args.thickness <= 0.0 {
        return Err(CommandError::invalid_arg(
            "thickness",
            "thickness must be a positive number",
        ));
    }
    check_distinct("ids", &args.ids)?;
    let targets: Vec<BrushObject> = args
        .ids
        .iter()
        .map(|&id| brush_of(ctx.doc(), id).cloned())
        .collect::<Result<_, _>>()?;

    let too_thick = |id: ObjectId| {
        CommandError::invalid_geometry(
            id,
            "the thickness leaves no room for a cavity",
            "thickness must be less than half the brush's smallest size",
        )
    };
    let mut walls = Vec::new();
    for t in &targets {
        let inner = match brush_inset(&t.brush, args.thickness) {
            Ok(inner) => inner,
            Err(BrushError::Empty | BrushError::Unbounded) => return Err(too_thick(t.id.into())),
            Err(e) => return Err(geometry_error(t.id, e)),
        };
        let fragments = brush_subtract(&t.brush, &inner).map_err(|e| geometry_error(t.id, e))?;
        walls.push(fragments);
    }

    let mut created = Vec::new();
    let mut map = Vec::new();
    for (t, fragments) in targets.iter().zip(walls) {
        let result = replace_with_fragments(ctx, t, fragments);
        created.extend(
            result
                .iter()
                .copied()
                .filter(|id| *id != ObjectId::from(t.id)),
        );
        map.push((t.id.into(), result));
    }
    Ok(Outcome::created_mapped(created, Vec::new(), map))
}
