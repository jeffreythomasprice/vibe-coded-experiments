use std::collections::{BTreeMap, BTreeSet};

use editor_core::{
    BrushId, BrushObject, ElementRef, FaceAttrs, FaceId, Object, ObjectId, Part, VertexId,
};
use editor_geom::tolerance::{WELD_EPS, snap_vec};
use editor_geom::uv;
use editor_geom::{VertexEditError, brush_geometry, move_brush_vertices};
use glam::{DVec2, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::brush::geometry_error;
use super::results;
use super::util::{brush_of, check_finite, mesh_of, mesh_problems, non_empty};
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
    let undefined = !material.is_empty() && ctx.doc().material_by_name(&material).is_none();
    let count = update_attrs(ctx, &args.faces, |a| a.material = material.clone())?;
    let mut out = Outcome::new(results::FacesChanged { faces: count });
    if undefined {
        out.warnings.push(format!(
            "material {material:?} is not defined; faces draw a flat colour until material.create adds it"
        ));
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UvMode {
    /// The given values replace the current ones.
    #[default]
    Absolute,
    /// `offset` is added, `scale` multiplies and `rotation` is added: for nudging.
    Relative,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetUv {
    pub faces: Vec<ElementRef>,
    /// In texels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<[f64; 2]>")]
    pub offset: Option<DVec2>,
    /// World units per texel; negative mirrors.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<[f64; 2]>")]
    pub scale: Option<DVec2>,
    /// Degrees, positive turns the image counter-clockwise seen from outside.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<UvMode>,
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
    let relative = args.mode == Some(UvMode::Relative);
    let count = update_attrs(ctx, &args.faces, |a| {
        if let Some(o) = args.offset {
            a.offset = if relative { a.offset + o } else { o };
        }
        if let Some(s) = args.scale {
            a.scale = if relative { a.scale * s } else { s };
        }
        if let Some(r) = args.rotation {
            a.rotation = if relative { a.rotation + r } else { r };
        }
    })?;
    Ok(Outcome::new(results::FacesChanged { faces: count }))
}

type BrushPicks = BTreeMap<BrushId, Vec<DVec3>>;

fn push_unique(list: &mut Vec<DVec3>, p: DVec3) {
    if !list.iter().any(|q| (*q - p).length() <= WELD_EPS) {
        list.push(p);
    }
}

fn vertex_error(id: BrushId, e: VertexEditError) -> CommandError {
    match e {
        VertexEditError::VertexNotFound(p) => {
            CommandError::invalid_arg("brush_vertices", format!("{id} has no vertex at {p}"))
                .with_hint("read the corner positions with scene.get at geometry detail")
        }
        VertexEditError::WouldBeConcave(_) => CommandError::invalid_geometry(
            id,
            e.to_string(),
            "brushes stay convex: move the vertex along its edges or outward, or convert with brush.to_mesh to edit a concave shape",
        ),
        VertexEditError::Brush(b) => geometry_error(id, b),
    }
}

fn move_brushes(
    ctx: &mut Ctx,
    picks: BrushPicks,
    delta: DVec3,
    lock: bool,
) -> CmdResult<results::VerticesMoved> {
    let mut updated = Vec::new();
    let mut moved = 0;
    for (id, positions) in picks {
        let object = brush_of(ctx.doc(), id.into())?;
        let brush = move_brush_vertices(&object.brush, &positions, delta, lock)
            .map_err(|e| vertex_error(id, e))?;
        moved += positions.len();
        updated.push(BrushObject {
            brush,
            ..object.clone()
        });
    }
    for brush in updated {
        ctx.insert(Object::Brush(brush));
    }
    Ok(Outcome::new(results::VerticesMoved { vertices: moved }))
}

fn move_vertices(
    ctx: &mut Ctx,
    picks: BTreeMap<ObjectId, BTreeSet<VertexId>>,
    delta: DVec3,
    lock: bool,
) -> CmdResult<results::VerticesMoved> {
    let mut warnings = Vec::new();
    let mut updated = Vec::new();
    let mut moved = 0;
    for (id, verts) in picks {
        let original = mesh_of(ctx.doc(), id)?.mesh.clone();
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
        if lock {
            uv::lock_mesh_faces(&original, &mut mesh.mesh);
        }
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

fn apply_moves(
    ctx: &mut Ctx,
    meshes: BTreeMap<ObjectId, BTreeSet<VertexId>>,
    brushes: BrushPicks,
    delta: DVec3,
    lock: bool,
) -> CmdResult<results::VerticesMoved> {
    let mut out = move_vertices(ctx, meshes, delta, lock)?;
    let brushes = move_brushes(ctx, brushes, delta, lock)?;
    out.result.vertices += brushes.result.vertices;
    out.warnings.extend(brushes.warnings);
    Ok(out)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranslateFaces {
    faces: Vec<ElementRef>,
    #[schemars(with = "[f64; 3]")]
    delta: DVec3,
    /// Keep face textures attached to the geometry (default true).
    #[serde(default)]
    texture_lock: Option<bool>,
}

pub fn translate_faces(ctx: &mut Ctx, args: TranslateFaces) -> CmdResult<results::VerticesMoved> {
    check_finite("delta", args.delta)?;
    let selection = group_faces(&args.faces)?;
    let mut mesh_picks: BTreeMap<ObjectId, BTreeSet<VertexId>> = BTreeMap::new();
    let mut brush_picks = BrushPicks::new();
    for (id, faces) in selection {
        if let ObjectId::Brush(b) = id {
            let object = brush_of(ctx.doc(), id)?;
            let geo = brush_geometry(&object.brush).map_err(|e| geometry_error(id, e))?;
            let positions = brush_picks.entry(b).or_default();
            let mut seen = BTreeSet::new();
            for f in &geo.faces {
                if faces.as_ref().is_none_or(|s| s.contains(&f.face)) {
                    for &i in &f.loop_ {
                        push_unique(positions, geo.vertices[i]);
                    }
                    seen.insert(f.face);
                }
            }
            if let Some(&missing) = faces.iter().flatten().find(|f| !seen.contains(f)) {
                return Err(missing_face(id, missing));
            }
            continue;
        }
        let mesh = &mesh_of(ctx.doc(), id)?.mesh;
        let set = mesh_picks.entry(id).or_default();
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
    apply_moves(
        ctx,
        mesh_picks,
        brush_picks,
        args.delta,
        args.texture_lock.unwrap_or(true),
    )
}

/// A brush corner, named by where it is now.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BrushVertexAt {
    pub brush: BrushId,
    #[schemars(with = "[f64; 3]")]
    pub at: DVec3,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TranslateVertices {
    /// Mesh vertices (`m3/v10`), or a bare mesh or brush id for all of its vertices.
    #[serde(default)]
    pub vertices: Vec<ElementRef>,
    /// Brush corners by world position (brush vertices have no ids); read them with scene.get at geometry detail.
    #[serde(default)]
    pub brush_vertices: Vec<BrushVertexAt>,
    #[schemars(with = "[f64; 3]")]
    pub delta: DVec3,
    /// Keep face textures attached to the geometry (default true).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub texture_lock: Option<bool>,
}

pub fn translate_vertices(
    ctx: &mut Ctx,
    args: TranslateVertices,
) -> CmdResult<results::VerticesMoved> {
    check_finite("delta", args.delta)?;
    if args.vertices.is_empty() && args.brush_vertices.is_empty() {
        return Err(CommandError::invalid_arg(
            "vertices",
            "give at least one of vertices and brush_vertices",
        ));
    }
    let mut mesh_picks: BTreeMap<ObjectId, BTreeSet<VertexId>> = BTreeMap::new();
    let mut brush_picks = BrushPicks::new();
    for r in &args.vertices {
        if let ObjectId::Brush(b) = r.object {
            let object = brush_of(ctx.doc(), r.object)?;
            match r.part {
                Part::All => {
                    let geo =
                        brush_geometry(&object.brush).map_err(|e| geometry_error(r.object, e))?;
                    let positions = brush_picks.entry(b).or_default();
                    for v in geo.vertices {
                        push_unique(positions, v);
                    }
                }
                _ => {
                    return Err(CommandError::invalid_arg(
                        "vertices",
                        format!("{r}: brush vertices have no ids"),
                    )
                    .with_hint("name them by position in brush_vertices"));
                }
            }
            continue;
        }
        let mesh = &mesh_of(ctx.doc(), r.object)?.mesh;
        let set = mesh_picks.entry(r.object).or_default();
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
    for pick in &args.brush_vertices {
        check_finite("brush_vertices", pick.at)?;
        brush_of(ctx.doc(), pick.brush.into())?;
        push_unique(brush_picks.entry(pick.brush).or_default(), pick.at);
    }
    apply_moves(
        ctx,
        mesh_picks,
        brush_picks,
        args.delta,
        args.texture_lock.unwrap_or(true),
    )
}
