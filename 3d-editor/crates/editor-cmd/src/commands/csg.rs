use editor_core::{Brush, BrushId, BrushObject, Mesh, MeshId, MeshObject, Object, ObjectId};
use editor_geom::mesh::{CsgError, subtract as mesh_subtract};
use editor_geom::{brush_subtract, brush_to_mesh};
use schemars::JsonSchema;
use serde::Deserialize;

use super::brush::geometry_error;
use super::results;
use super::util::{Parents, brush_or_mesh};
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
    Ok(ids)
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
