use std::collections::{BTreeSet, HashMap};

use level_core::{ElementRef, ObjectId, Part, PropertyValue};
use level_geom::mesh::{MeshIssue, Severity, validate};
use level_geom::{BrushError, brush_geometry};
use schemars::JsonSchema;
use serde::Deserialize;

use super::results;
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Validate {
    ids: Option<Vec<ObjectId>>,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub code: &'static str,
    pub severity: &'static str,
    pub ids: Vec<ObjectId>,
    pub elements: Vec<ElementRef>,
    pub message: String,
    pub hint: &'static str,
}

fn severity(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
    }
}

fn mesh_issue(id: ObjectId, issue: &MeshIssue) -> Issue {
    let (code, hint) = match issue {
        MeshIssue::OpenEdge { .. } => (
            "mesh_open",
            "close the hole by adding faces, or ignore if the mesh is meant to be open",
        ),
        MeshIssue::NonManifoldEdge { .. } | MeshIssue::InconsistentWinding { .. } => (
            "mesh_nonmanifold",
            "make each edge shared by exactly two faces with opposite winding",
        ),
        MeshIssue::NonPlanarFace { .. } => (
            "mesh_nonplanar_face",
            "move the face vertices onto one plane or split the face",
        ),
        MeshIssue::ZeroAreaFace { .. } => (
            "degenerate_face",
            "delete the face or move its vertices apart",
        ),
        _ => (
            "invalid_mesh",
            "fix or delete the referenced face or vertex",
        ),
    };
    let face = |f: level_core::FaceId| ElementRef {
        object: id,
        part: Part::Face(f),
    };
    let vertex = |v: level_core::VertexId| ElementRef {
        object: id,
        part: Part::Vertex(v),
    };
    let (elements, detail) = match issue {
        MeshIssue::DanglingVertex { face: f, vertex: v } => (
            vec![face(*f), vertex(*v)],
            format!("{} uses missing vertex {}", face(*f), vertex(*v)),
        ),
        MeshIssue::FaceTooSmall { face: f } => (
            vec![face(*f)],
            format!("{} has fewer than 3 vertices", face(*f)),
        ),
        MeshIssue::DuplicateVertexInFace { face: f, vertex: v } => (
            vec![face(*f), vertex(*v)],
            format!("{} repeats vertex {}", face(*f), vertex(*v)),
        ),
        MeshIssue::ZeroAreaFace { face: f } => {
            (vec![face(*f)], format!("{} has no area", face(*f)))
        }
        MeshIssue::NonPlanarFace { face: f, deviation } => (
            vec![face(*f)],
            format!("{} is not planar (off by {deviation:.3})", face(*f)),
        ),
        MeshIssue::OpenEdge { a, b } => (
            vec![vertex(*a), vertex(*b)],
            format!("open edge {}-{}", vertex(*a), vertex(*b)),
        ),
        MeshIssue::NonManifoldEdge {
            a,
            b,
            forward,
            backward,
        } => (
            vec![vertex(*a), vertex(*b)],
            format!(
                "edge {}-{} is shared by {forward} forward and {backward} backward faces",
                vertex(*a),
                vertex(*b)
            ),
        ),
        MeshIssue::InconsistentWinding { a, b } => (
            vec![vertex(*a), vertex(*b)],
            format!(
                "faces on edge {}-{} wind the same way",
                vertex(*a),
                vertex(*b)
            ),
        ),
        MeshIssue::UnusedVertex { vertex: v } => (
            vec![vertex(*v)],
            format!("{} belongs to no face", vertex(*v)),
        ),
        MeshIssue::InsideOut => (vec![], "faces point inward".to_string()),
    };
    Issue {
        code,
        severity: severity(issue.severity()),
        ids: vec![id],
        elements,
        message: format!("{id}: {detail}"),
        hint,
    }
}

pub fn validate_doc(ctx: &Ctx) -> Vec<Issue> {
    let doc = ctx.doc();
    let mut issues = Vec::new();

    let mut shapes: HashMap<Vec<[i64; 3]>, Vec<ObjectId>> = HashMap::new();
    for b in doc.brushes.values() {
        match brush_geometry(&b.brush) {
            Ok(geo) => {
                let mut key: Vec<[i64; 3]> = geo
                    .vertices
                    .iter()
                    .map(|v| {
                        [
                            (v.x * 1024.0).round() as i64,
                            (v.y * 1024.0).round() as i64,
                            (v.z * 1024.0).round() as i64,
                        ]
                    })
                    .collect();
                key.sort_unstable();
                shapes.entry(key).or_default().push(b.id.into());
            }
            Err(e) => {
                let (code, hint) = match e {
                    BrushError::DegeneratePlane(_) => (
                        "degenerate_face",
                        "give the face three non-collinear plane points",
                    ),
                    _ => ("invalid_brush", "recreate the brush or fix its face planes"),
                };
                issues.push(Issue {
                    code,
                    severity: "error",
                    ids: vec![b.id.into()],
                    elements: vec![],
                    message: format!("{}: {e}", b.id),
                    hint,
                });
            }
        }
    }
    for ids in shapes.into_values().filter(|ids| ids.len() > 1) {
        issues.push(Issue {
            code: "duplicate_object",
            severity: "warning",
            message: format!(
                "{} occupy the same space",
                ids.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ids,
            elements: vec![],
            hint: "delete the duplicates",
        });
    }

    for m in doc.meshes.values() {
        for issue in validate(&m.mesh) {
            issues.push(mesh_issue(m.id.into(), &issue));
        }
    }

    let targetnames: BTreeSet<&str> = doc
        .entities
        .values()
        .filter_map(|e| match e.properties.get("targetname") {
            Some(PropertyValue::String(s)) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    for e in doc.entities.values() {
        if e.origin.is_none() && doc.children(e.id.into()).is_empty() {
            issues.push(Issue {
                code: "empty_brush_entity",
                severity: "warning",
                ids: vec![e.id.into()],
                elements: vec![],
                message: format!("{} ({}) owns no brushes or meshes", e.id, e.classname),
                hint: "assign geometry to the entity or delete it",
            });
        }
        if let Some(PropertyValue::String(t)) = e.properties.get("target")
            && !targetnames.contains(t.as_str())
        {
            issues.push(Issue {
                code: "missing_target",
                severity: "warning",
                ids: vec![e.id.into()],
                elements: vec![],
                message: format!("{} targets \"{t}\" but no entity has that targetname", e.id),
                hint: "set targetname on the entity it should point at, or fix the target",
            });
        }
    }
    issues
}

pub fn validate_cmd(ctx: &Ctx, args: Validate) -> CmdResult<results::Validation> {
    let filter: Option<BTreeSet<ObjectId>> = match args.ids {
        None => None,
        Some(ids) => {
            for &id in &ids {
                if !ctx.doc().contains(id) {
                    return Err(CommandError::not_found(id));
                }
            }
            Some(ids.into_iter().collect())
        }
    };
    let mut issues = validate_doc(ctx);
    if let Some(f) = &filter {
        issues.retain(|i| i.ids.iter().any(|id| f.contains(id)));
    }
    let errors = issues.iter().filter(|i| i.severity == "error").count();
    let warnings = issues.len() - errors;
    Ok(Outcome::new(results::Validation {
        issues: issues.into_iter().map(results::IssueView::from).collect(),
        errors,
        warnings,
    }))
}
