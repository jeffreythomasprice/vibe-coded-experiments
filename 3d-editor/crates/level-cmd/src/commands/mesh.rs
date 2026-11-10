use std::collections::{BTreeMap, BTreeSet};

use glam::DVec3;
use level_core::{
    BrushId, BrushObject, ElementRef, FaceAttrs, FaceId, LayerId, Mesh, MeshFace, MeshId,
    MeshObject, MeshVertex, Object, ObjectId, Part, VertexId,
};
use level_geom::mesh::{MeshIssue, validate};
use level_geom::tolerance::snap_vec;
use level_geom::{MeshToBrushError, brush_to_mesh, mesh_to_brush};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::brush::geometry_error;
use super::util::{brush_of, check_finite, mesh_of, mesh_problems, non_empty, resolve_layer};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FaceSpec {
    /// Zero-based indices into `vertices`, counter-clockwise seen from outside.
    verts: Vec<usize>,
    #[serde(default)]
    material: String,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Create {
    #[schemars(with = "Vec<[f64; 3]>")]
    vertices: Vec<DVec3>,
    faces: Vec<FaceSpec>,
    layer: Option<LayerId>,
}

pub fn create(ctx: &mut Ctx, args: Create) -> CmdResult {
    non_empty("faces", &args.faces)?;
    if args.vertices.len() < 3 {
        return Err(CommandError::invalid_arg(
            "vertices",
            "a mesh needs at least 3 vertices",
        ));
    }
    let layer = resolve_layer(ctx.doc(), args.layer)?;
    let mut mesh = Mesh::default();
    for (i, &p) in args.vertices.iter().enumerate() {
        check_finite("vertices", p)?;
        mesh.vertices.push(MeshVertex {
            id: VertexId(i as u32 + 1),
            pos: snap_vec(p),
        });
    }
    for (i, spec) in args.faces.iter().enumerate() {
        let mut verts = Vec::with_capacity(spec.verts.len());
        for &index in &spec.verts {
            if index >= args.vertices.len() {
                return Err(CommandError::invalid_arg(
                    "faces",
                    format!(
                        "face {i} uses vertex index {index}, but there are only {} vertices",
                        args.vertices.len()
                    ),
                )
                .with_hint("indices are zero-based"));
            }
            verts.push(VertexId(index as u32 + 1));
        }
        mesh.faces.push(MeshFace {
            id: FaceId(i as u32 + 1),
            verts,
            attrs: FaceAttrs::with_material(&spec.material),
        });
    }
    let id = MeshId(ctx.alloc_id());
    let warnings = mesh_problems(id.into(), &mesh)?;
    ctx.insert(Object::Mesh(MeshObject {
        id,
        layer,
        group: None,
        entity: None,
        mesh,
        extra: Default::default(),
    }));
    Ok(Outcome::created(vec![id.into()], warnings))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DeleteFaces {
    faces: Vec<ElementRef>,
}

fn is_open(mesh: &Mesh) -> bool {
    validate(mesh)
        .iter()
        .any(|i| matches!(i, MeshIssue::OpenEdge { .. }))
}

pub fn delete_faces(ctx: &mut Ctx, args: DeleteFaces) -> CmdResult {
    non_empty("faces", &args.faces)?;
    let mut doomed: BTreeMap<ObjectId, BTreeSet<FaceId>> = BTreeMap::new();
    for r in &args.faces {
        match r.part {
            Part::Face(f) => {
                doomed.entry(r.object).or_default().insert(f);
            }
            _ => {
                return Err(CommandError::invalid_arg(
                    "faces",
                    format!("{r} does not name a face; faces look like m3/f2"),
                )
                .with_hint("use object.delete to remove a whole mesh"));
            }
        }
    }

    let mut warnings = Vec::new();
    let mut updated = Vec::new();
    let mut removed = 0;
    for (id, faces) in doomed {
        let object = mesh_of(ctx.doc(), id)?;
        if let Some(&missing) = faces
            .iter()
            .find(|f| !object.mesh.faces.iter().any(|x| x.id == **f))
        {
            return Err(CommandError::not_found(ElementRef {
                object: id,
                part: Part::Face(missing),
            }));
        }
        if faces.len() == object.mesh.faces.len() {
            return Err(CommandError::invalid_arg(
                "faces",
                format!("that would remove every face of {id}"),
            )
            .with_hint("use object.delete to remove the mesh"));
        }
        let was_open = is_open(&object.mesh);
        let mut mesh = object.clone();
        mesh.mesh.faces.retain(|f| !faces.contains(&f.id));
        let used: BTreeSet<VertexId> = mesh
            .mesh
            .faces
            .iter()
            .flat_map(|f| f.verts.iter().copied())
            .collect();
        mesh.mesh.vertices.retain(|v| used.contains(&v.id));
        removed += faces.len();
        if !was_open && is_open(&mesh.mesh) {
            warnings.push(format!("{id} is no longer closed"));
        }
        updated.push(mesh);
    }
    for mesh in updated {
        ctx.insert(Object::Mesh(mesh));
    }
    Ok(Outcome {
        result: json!({ "removed": removed }),
        warnings,
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Convert {
    ids: Vec<ObjectId>,
}

pub fn mesh_to_brush_cmd(ctx: &mut Ctx, args: Convert) -> CmdResult {
    non_empty("ids", &args.ids)?;
    let mut converted = Vec::new();
    for &id in &args.ids {
        let source = mesh_of(ctx.doc(), id)?.clone();
        let brush = mesh_to_brush(&source.mesh).map_err(|e| match e {
            MeshToBrushError::Brush(b) => geometry_error(id, b),
            other => CommandError::invalid_geometry(
                id,
                other.to_string(),
                "only closed convex meshes convert to brushes; keep it as a mesh otherwise",
            ),
        })?;
        converted.push((source, brush));
    }
    let mut created = Vec::new();
    let mut map = Vec::new();
    for (source, brush) in converted {
        let new_id = BrushId(ctx.alloc_id());
        ctx.remove(source.id.into());
        ctx.insert(Object::Brush(BrushObject {
            id: new_id,
            layer: source.layer,
            group: source.group,
            entity: source.entity,
            brush,
            extra: source.extra,
        }));
        map.push((source.id.into(), vec![new_id.into()]));
        created.push(new_id.into());
    }
    Ok(Outcome::created(created, vec![]).with_map(map))
}

pub fn brush_to_mesh_cmd(ctx: &mut Ctx, args: Convert) -> CmdResult {
    non_empty("ids", &args.ids)?;
    let mut converted = Vec::new();
    for &id in &args.ids {
        let source = brush_of(ctx.doc(), id)?.clone();
        let mesh = brush_to_mesh(&source.brush).map_err(|e| geometry_error(id, e))?;
        converted.push((source, mesh));
    }
    let mut created = Vec::new();
    let mut map = Vec::new();
    for (source, mesh) in converted {
        let new_id = MeshId(ctx.alloc_id());
        ctx.remove(source.id.into());
        ctx.insert(Object::Mesh(MeshObject {
            id: new_id,
            layer: source.layer,
            group: source.group,
            entity: source.entity,
            mesh,
            extra: source.extra,
        }));
        map.push((source.id.into(), vec![new_id.into()]));
        created.push(new_id.into());
    }
    Ok(Outcome::created(created, vec![]).with_map(map))
}
