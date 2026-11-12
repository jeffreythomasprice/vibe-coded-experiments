use std::collections::{BTreeSet, HashMap};

use editor_core::{ElementRef, ObjectId, Part};
use editor_geom::mesh::{MeshIssue, Severity, validate};
use editor_geom::{BrushError, brush_geometry};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results;
use super::{CmdResult, Outcome};
use crate::component_defs::{Catalog, ComponentIssue, DefsProblem};
use crate::ctx::Ctx;
use crate::error::CommandError;
use crate::links::{LinkStatus, status_of};

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Validate {
    ids: Option<Vec<ObjectId>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IssueCode {
    InvalidBrush,
    InvalidMesh,
    DegenerateFace,
    MeshOpen,
    MeshNonmanifold,
    MeshNonplanarFace,
    DuplicateObject,
    TextureUnreadable,
    UndefinedMaterial,
    DefsLoadFailed,
    DefsBadSchema,
    DuplicateComponent,
    UnknownComponent,
    SchemaViolation,
    DanglingLink,
    LinkNotEntity,
    SelfLink,
}

#[derive(Debug, Clone)]
pub struct Issue {
    pub code: IssueCode,
    pub severity: IssueSeverity,
    pub ids: Vec<ObjectId>,
    pub elements: Vec<ElementRef>,
    pub message: String,
    pub hint: &'static str,
}

fn severity(s: Severity) -> IssueSeverity {
    match s {
        Severity::Error => IssueSeverity::Error,
        Severity::Warning => IssueSeverity::Warning,
    }
}

fn mesh_issue(id: ObjectId, issue: &MeshIssue) -> Issue {
    let (code, hint) = match issue {
        MeshIssue::OpenEdge { .. } => (
            IssueCode::MeshOpen,
            "close the hole by adding faces, or ignore if the mesh is meant to be open",
        ),
        MeshIssue::NonManifoldEdge { .. } | MeshIssue::InconsistentWinding { .. } => (
            IssueCode::MeshNonmanifold,
            "make each edge shared by exactly two faces with opposite winding",
        ),
        MeshIssue::NonPlanarFace { .. } => (
            IssueCode::MeshNonplanarFace,
            "move the face vertices onto one plane or split the face",
        ),
        MeshIssue::ZeroAreaFace { .. } => (
            IssueCode::DegenerateFace,
            "delete the face or move its vertices apart",
        ),
        _ => (
            IssueCode::InvalidMesh,
            "fix or delete the referenced face or vertex",
        ),
    };
    let face = |f: editor_core::FaceId| ElementRef {
        object: id,
        part: Part::Face(f),
    };
    let vertex = |v: editor_core::VertexId| ElementRef {
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
                        IssueCode::DegenerateFace,
                        "give the face three non-collinear plane points",
                    ),
                    _ => (
                        IssueCode::InvalidBrush,
                        "recreate the brush or fix its face planes",
                    ),
                };
                issues.push(Issue {
                    code,
                    severity: IssueSeverity::Error,
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
            code: IssueCode::DuplicateObject,
            severity: IssueSeverity::Warning,
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

    for m in doc.materials.values() {
        if let Some(source) = &m.texture
            && let Err(e) = crate::texture::source_size(source, ctx.scene_dir())
        {
            issues.push(Issue {
                code: IssueCode::TextureUnreadable,
                severity: IssueSeverity::Error,
                ids: vec![m.id.into()],
                elements: vec![],
                message: format!("{} ({:?}): {e}", m.id, m.name),
                hint: "fix the path with material.set, or re-create the image file",
            });
        }
    }
    let mut undefined: std::collections::BTreeMap<&str, Vec<ObjectId>> = Default::default();
    for (id, material) in doc
        .brushes
        .values()
        .flat_map(|b| {
            b.brush
                .faces
                .iter()
                .map(move |f| (ObjectId::from(b.id), f.attrs.material.as_str()))
        })
        .chain(doc.meshes.values().flat_map(|m| {
            m.mesh
                .faces
                .iter()
                .map(move |f| (ObjectId::from(m.id), f.attrs.material.as_str()))
        }))
    {
        if !material.is_empty() && doc.material_by_name(material).is_none() {
            let ids = undefined.entry(material).or_default();
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    for (name, ids) in undefined {
        issues.push(Issue {
            code: IssueCode::UndefinedMaterial,
            severity: IssueSeverity::Warning,
            message: format!(
                "material {name:?} is used by {} but not defined",
                ids.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            ids,
            elements: vec![],
            hint: "define it with material.create, or change the faces with face.set_material",
        });
    }

    let catalog = Catalog::resolve(doc, ctx.scene_dir());
    for problem in catalog.problems() {
        let (code, severity, hint) = match problem {
            DefsProblem::Unreadable { .. }
            | DefsProblem::Malformed { .. }
            | DefsProblem::UnsupportedVersion { .. } => (
                IssueCode::DefsLoadFailed,
                IssueSeverity::Error,
                "fix the path or file, or re-add it with component_defs.add",
            ),
            DefsProblem::BadSchema { .. } => (
                IssueCode::DefsBadSchema,
                IssueSeverity::Error,
                "fix the component schema in the definitions file",
            ),
            DefsProblem::DuplicateComponent { .. } => (
                IssueCode::DuplicateComponent,
                IssueSeverity::Warning,
                "remove one of the definitions; a built-in wins, otherwise the later record",
            ),
        };
        issues.push(Issue {
            code,
            severity,
            ids: vec![problem.from().into()],
            elements: vec![],
            message: problem.to_string(),
            hint,
        });
    }

    for e in doc.entities.values() {
        for (name, data) in &e.components {
            for found in catalog.component_issues(name, data) {
                let (code, severity, hint) = match &found {
                    ComponentIssue::Unknown { .. } => (
                        IssueCode::UnknownComponent,
                        IssueSeverity::Warning,
                        "define the component with component_defs.add, or remove it",
                    ),
                    ComponentIssue::Schema(_) => (
                        IssueCode::SchemaViolation,
                        IssueSeverity::Error,
                        "fix the value with entity.set_component or entity.update_component",
                    ),
                };
                issues.push(Issue {
                    code,
                    severity,
                    ids: vec![e.id.into()],
                    elements: vec![],
                    message: format!("{}: {name}: {found}", e.id),
                    hint,
                });
            }
        }
        for (name, data) in &e.components {
            for link in catalog.links(name, data) {
                let (status, to) = status_of(doc, &link.value);
                let (code, hint, what) = match status {
                    LinkStatus::Ok if to == Some(e.id) => (
                        IssueCode::SelfLink,
                        "point the link at another entity",
                        "itself",
                    ),
                    LinkStatus::Ok => continue,
                    LinkStatus::Missing | LinkStatus::Malformed => (
                        IssueCode::DanglingLink,
                        "set the link to the id of an existing entity, or remove it",
                        "nothing: no such entity",
                    ),
                    LinkStatus::NotEntity => (
                        IssueCode::LinkNotEntity,
                        "links point at entities (e ids), not brushes, meshes or groups",
                        "an object that is not an entity",
                    ),
                };
                issues.push(Issue {
                    code,
                    severity: IssueSeverity::Warning,
                    ids: vec![e.id.into()],
                    elements: vec![],
                    message: format!(
                        "{} link {name}{} = \"{}\" points at {what}",
                        e.id, link.pointer, link.value
                    ),
                    hint,
                });
            }
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
    let errors = issues
        .iter()
        .filter(|i| i.severity == IssueSeverity::Error)
        .count();
    let warnings = issues.len() - errors;
    Ok(Outcome::new(results::Validation {
        issues: issues.into_iter().map(results::IssueView::from).collect(),
        errors,
        warnings,
    }))
}
