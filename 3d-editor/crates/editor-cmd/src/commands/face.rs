use std::collections::{BTreeMap, BTreeSet};

use editor_core::{ElementRef, FaceAttrs, FaceId, Object, ObjectId, Part, VertexId};
use editor_geom::tolerance::snap_vec;
use glam::{DVec2, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results;
use super::util::{check_finite, mesh_of, mesh_problems, non_empty};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

/// `None` means every face of the object.
type FaceSelection = BTreeMap<ObjectId, Option<BTreeSet<FaceId>>>;

fn group_faces(refs: &[ElementRef]) -> Result<FaceSelection, CommandError> {
    non_empty("faces", refs)?;
    let mut out: FaceSelection = BTreeMap::new();
    for r in refs {
        match r.part {
            Part::All => {
                out.insert(r.object, None);
            }
            Part::Face(f) => {
                if let Some(set) = out.entry(r.object).or_insert_with(|| Some(BTreeSet::new())) {
                    set.insert(f);
                }
            }
            Part::Vertex(_) => {
                return Err(CommandError::invalid_arg(
                    "faces",
                    format!("{r} is a vertex; faces look like b12/f3"),
                ));
            }
        }
    }
    Ok(out)
}

fn missing_face(object: ObjectId, face: FaceId) -> CommandError {
    CommandError::not_found(ElementRef {
        object,
        part: Part::Face(face),
    })
}

fn update_attrs(
    ctx: &mut Ctx,
    refs: &[ElementRef],
    edit: impl Fn(&mut FaceAttrs),
) -> Result<usize, CommandError> {
    let selection = group_faces(refs)?;
    let mut updated = Vec::new();
    let mut count = 0;
    for (id, faces) in selection {
        let mut object = super::util::brush_or_mesh(ctx.doc(), id)?;
        let mut seen = BTreeSet::new();
        let mut visit = |face: FaceId, attrs: &mut FaceAttrs| {
            if faces.as_ref().is_none_or(|set| set.contains(&face)) {
                edit(attrs);
                seen.insert(face);
                count += 1;
            }
        };
        match &mut object {
            Object::Brush(b) => b
                .brush
                .faces
                .iter_mut()
                .for_each(|f| visit(f.id, &mut f.attrs)),
            Object::Mesh(m) => m
                .mesh
                .faces
                .iter_mut()
                .for_each(|f| visit(f.id, &mut f.attrs)),
            _ => unreachable!("brush_or_mesh returned another kind"),
        }
        if let Some(wanted) = &faces
            && let Some(&missing) = wanted.iter().find(|f| !seen.contains(f))
        {
            return Err(missing_face(id, missing));
        }
        updated.push(object);
    }
    for object in updated {
        ctx.insert(object);
    }
    Ok(count)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetMaterial {
    pub faces: Vec<ElementRef>,
    pub material: String,
}

pub fn set_material(ctx: &mut Ctx, args: SetMaterial) -> CmdResult<results::FacesChanged> {
    let material = args.material;
    let count = update_attrs(ctx, &args.faces, |a| a.material = material.clone())?;
    Ok(Outcome::new(results::FacesChanged { faces: count }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetUv {
    faces: Vec<ElementRef>,
    #[schemars(with = "Option<[f64; 2]>")]
    offset: Option<DVec2>,
    #[schemars(with = "Option<[f64; 2]>")]
    scale: Option<DVec2>,
    rotation: Option<f64>,
}

pub fn set_uv(ctx: &mut Ctx, args: SetUv) -> CmdResult<results::FacesChanged> {
    if args.offset.is_none() && args.scale.is_none() && args.rotation.is_none() {
        return Err(CommandError::invalid_arg(
            "offset",
            "give at least one of offset, scale and rotation",
        ));
    }
    if let Some(o) = args.offset
        && !o.is_finite()
    {
        return Err(CommandError::invalid_arg("offset", "offset must be finite"));
    }
    if let Some(s) = args.scale
        && (!s.is_finite() || s.abs().min_element() < 1e-9)
    {
        return Err(CommandError::invalid_arg(
            "scale",
            "scale must be finite and non-zero on both axes",
        ));
    }
    if let Some(r) = args.rotation
        && !r.is_finite()
    {
        return Err(CommandError::invalid_arg(
            "rotation",
            "rotation must be finite",
        ));
    }
    let count = update_attrs(ctx, &args.faces, |a| {
        if let Some(o) = args.offset {
            a.offset = o;
        }
        if let Some(s) = args.scale {
            a.scale = s;
        }
        if let Some(r) = args.rotation {
            a.rotation = r;
        }
    })?;
    Ok(Outcome::new(results::FacesChanged { faces: count }))
}

fn mesh_only(ctx: &Ctx, id: ObjectId) -> Result<(), CommandError> {
    match id {
        ObjectId::Brush(_) if ctx.doc().contains(id) => Err(CommandError::invalid_arg(
            "faces",
            format!("{id} is a brush; moving brush faces and vertices arrives in Tier 1"),
        )
        .with_hint("convert it with brush.to_mesh first, or use transform.* / brush.clip")),
        _ => mesh_of(ctx.doc(), id).map(|_| ()),
    }
}

fn move_vertices(
    ctx: &mut Ctx,
    picks: BTreeMap<ObjectId, BTreeSet<VertexId>>,
    delta: DVec3,
) -> CmdResult<results::VerticesMoved> {
    let mut warnings = Vec::new();
    let mut updated = Vec::new();
    let mut moved = 0;
    for (id, verts) in picks {
        let mut mesh = mesh_of(ctx.doc(), id)?.clone();
        for v in &mut mesh.mesh.vertices {
            if verts.contains(&v.id) {
                let pos = snap_vec(v.pos + delta);
                check_finite("delta", pos)?;
                v.pos = pos;
                moved += 1;
            }
        }
        warnings.extend(mesh_problems(id, &mesh.mesh)?);
        updated.push(mesh);
    }
    for mesh in updated {
        ctx.insert(Object::Mesh(mesh));
    }
    Ok(Outcome {
        result: results::VerticesMoved { vertices: moved },
        warnings,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranslateFaces {
    faces: Vec<ElementRef>,
    #[schemars(with = "[f64; 3]")]
    delta: DVec3,
}

pub fn translate_faces(ctx: &mut Ctx, args: TranslateFaces) -> CmdResult<results::VerticesMoved> {
    check_finite("delta", args.delta)?;
    let selection = group_faces(&args.faces)?;
    let mut picks: BTreeMap<ObjectId, BTreeSet<VertexId>> = BTreeMap::new();
    for (id, faces) in selection {
        mesh_only(ctx, id)?;
        let mesh = &mesh_of(ctx.doc(), id)?.mesh;
        let set = picks.entry(id).or_default();
        let mut seen = BTreeSet::new();
        for f in &mesh.faces {
            if faces.as_ref().is_none_or(|s| s.contains(&f.id)) {
                set.extend(f.verts.iter().copied());
                seen.insert(f.id);
            }
        }
        if let Some(&missing) = faces.iter().flatten().find(|f| !seen.contains(f)) {
            return Err(missing_face(id, missing));
        }
    }
    move_vertices(ctx, picks, args.delta)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranslateVertices {
    vertices: Vec<ElementRef>,
    #[schemars(with = "[f64; 3]")]
    delta: DVec3,
}

pub fn translate_vertices(
    ctx: &mut Ctx,
    args: TranslateVertices,
) -> CmdResult<results::VerticesMoved> {
    check_finite("delta", args.delta)?;
    non_empty("vertices", &args.vertices)?;
    let mut picks: BTreeMap<ObjectId, BTreeSet<VertexId>> = BTreeMap::new();
    for r in &args.vertices {
        mesh_only(ctx, r.object)?;
        let mesh = &mesh_of(ctx.doc(), r.object)?.mesh;
        let set = picks.entry(r.object).or_default();
        match r.part {
            Part::All => set.extend(mesh.vertices.iter().map(|v| v.id)),
            Part::Vertex(v) => {
                if mesh.vertex(v).is_none() {
                    return Err(CommandError::not_found(r));
                }
                set.insert(v);
            }
            Part::Face(_) => {
                return Err(CommandError::invalid_arg(
                    "vertices",
                    format!("{r} is a face; vertices look like m3/v10"),
                ));
            }
        }
    }
    move_vertices(ctx, picks, args.delta)
}
