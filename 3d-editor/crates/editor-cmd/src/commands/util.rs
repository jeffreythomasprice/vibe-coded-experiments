use editor_core::{BrushObject, Document, EntityId, Extra, GroupId, LayerId, Object, ObjectId};
use glam::DVec3;

use crate::error::CommandError;

pub fn brush_of(doc: &Document, id: ObjectId) -> Result<&BrushObject, CommandError> {
    match id {
        ObjectId::Brush(b) => doc
            .brushes
            .get(&b)
            .ok_or_else(|| CommandError::not_found(id)),
        _ if doc.contains(id) => Err(CommandError::wrong_kind(id, "brush")),
        _ => Err(CommandError::not_found(id)),
    }
}

pub fn brush_or_mesh(doc: &Document, id: ObjectId) -> Result<Object, CommandError> {
    match id {
        ObjectId::Brush(_) | ObjectId::Mesh(_) => {
            doc.get(id).ok_or_else(|| CommandError::not_found(id))
        }
        _ if doc.contains(id) => Err(CommandError::wrong_kind(id, "brush or mesh")),
        _ => Err(CommandError::not_found(id)),
    }
}

pub fn resolve_layer(doc: &Document, layer: Option<LayerId>) -> Result<LayerId, CommandError> {
    match layer {
        Some(l) if doc.layers.contains_key(&l) => Ok(l),
        Some(l) => Err(CommandError::not_found(l)),
        None => doc
            .layers
            .keys()
            .next()
            .copied()
            .ok_or_else(|| CommandError::invalid_arg("layer", "the document has no layers")),
    }
}

pub fn check_finite(field: &str, v: DVec3) -> Result<(), CommandError> {
    if !v.is_finite() {
        return Err(CommandError::invalid_arg(
            field,
            "values must be finite numbers",
        ));
    }
    if v.abs().max_element() > editor_geom::tolerance::WORLD_BOUND {
        return Err(CommandError::invalid_arg(
            field,
            format!(
                "coordinates must stay within ±{}",
                editor_geom::tolerance::WORLD_BOUND
            ),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct Parents {
    pub layer: LayerId,
    pub group: Option<GroupId>,
    pub entity: Option<EntityId>,
    pub extra: Extra,
    pub flags: editor_core::Flags,
}

impl Parents {
    pub fn of_brush(b: &BrushObject) -> Self {
        Self {
            layer: b.layer,
            group: b.group,
            entity: b.entity,
            extra: b.extra.clone(),
            flags: b.flags,
        }
    }
}

pub fn renumber(id: ObjectId, number: u64) -> ObjectId {
    use editor_core::{BrushId, CameraId, MaterialId, MeshId};
    match id {
        ObjectId::Brush(_) => BrushId(number).into(),
        ObjectId::Mesh(_) => MeshId(number).into(),
        ObjectId::Entity(_) => EntityId(number).into(),
        ObjectId::Group(_) => GroupId(number).into(),
        ObjectId::Layer(_) => LayerId(number).into(),
        ObjectId::Camera(_) => CameraId(number).into(),
        ObjectId::Material(_) => MaterialId(number).into(),
    }
}

/// The object itself plus everything below it (group members, brush entity members).
pub fn descendants(doc: &Document, id: ObjectId, out: &mut std::collections::BTreeSet<ObjectId>) {
    if !out.insert(id) {
        return;
    }
    if matches!(id, ObjectId::Entity(_) | ObjectId::Group(_)) {
        for child in doc.children(id) {
            descendants(doc, child, out);
        }
    }
}

/// Brushes, meshes, entities and groups: what can be moved, copied, grouped and selected.
pub fn world_object(doc: &Document, id: ObjectId) -> Result<Object, CommandError> {
    match id {
        ObjectId::Brush(_) | ObjectId::Mesh(_) | ObjectId::Entity(_) | ObjectId::Group(_) => {
            doc.get(id).ok_or_else(|| CommandError::not_found(id))
        }
        _ if doc.contains(id) => Err(CommandError::wrong_kind(id, "brush, mesh, entity or group")),
        _ => Err(CommandError::not_found(id)),
    }
}

pub fn non_empty<T>(field: &str, items: &[T]) -> Result<(), CommandError> {
    if items.is_empty() {
        Err(CommandError::invalid_arg(
            field,
            format!("{field} must not be empty"),
        ))
    } else {
        Ok(())
    }
}

pub fn with_layer(object: Object, layer: LayerId) -> Object {
    match object {
        Object::Brush(mut o) => {
            o.layer = layer;
            Object::Brush(o)
        }
        Object::Mesh(mut o) => {
            o.layer = layer;
            Object::Mesh(o)
        }
        Object::Entity(mut o) => {
            o.layer = layer;
            Object::Entity(o)
        }
        Object::Group(mut o) => {
            o.layer = layer;
            Object::Group(o)
        }
        other => other,
    }
}

/// Sets the owning group (the parent, for a group). Layers and cameras are returned unchanged.
pub fn with_group(object: Object, group: Option<GroupId>) -> Object {
    match object {
        Object::Brush(mut o) => {
            o.group = group;
            Object::Brush(o)
        }
        Object::Mesh(mut o) => {
            o.group = group;
            Object::Mesh(o)
        }
        Object::Entity(mut o) => {
            o.group = group;
            Object::Entity(o)
        }
        Object::Group(mut o) => {
            o.parent = group;
            Object::Group(o)
        }
        other => other,
    }
}

pub fn with_entity(object: Object, entity: Option<EntityId>) -> Object {
    match object {
        Object::Brush(mut o) => {
            o.entity = entity;
            Object::Brush(o)
        }
        Object::Mesh(mut o) => {
            o.entity = entity;
            Object::Mesh(o)
        }
        other => other,
    }
}

/// Moves an object into `group`; a brush entity takes its members along.
pub fn set_group(ctx: &mut crate::ctx::Ctx, id: ObjectId, group: Option<GroupId>) {
    let Some(object) = ctx.doc().get(id) else {
        return;
    };
    ctx.insert(with_group(object, group));
    if matches!(id, ObjectId::Entity(_)) {
        for child in ctx.doc().children(id) {
            if let Some(c) = ctx.doc().get(child) {
                ctx.insert(with_group(c, group));
            }
        }
    }
}

pub const RESERVED_PROPERTIES: &[&str] = &["classname", "origin", "angles"];

#[derive(Debug, Clone, Copy, serde::Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BoxArg {
    #[schemars(with = "[f64; 3]")]
    pub min: DVec3,
    #[schemars(with = "[f64; 3]")]
    pub max: DVec3,
}

impl BoxArg {
    pub fn aabb(&self, field: &str) -> Result<editor_geom::Aabb, CommandError> {
        check_finite(field, self.min)?;
        check_finite(field, self.max)?;
        if !self.min.cmple(self.max).all() {
            return Err(CommandError::invalid_arg(
                field,
                "min must not exceed max on any axis",
            ));
        }
        Ok(editor_geom::Aabb {
            min: self.min,
            max: self.max,
        })
    }
}

pub fn mesh_of(doc: &Document, id: ObjectId) -> Result<&editor_core::MeshObject, CommandError> {
    match id {
        ObjectId::Mesh(m) => doc
            .meshes
            .get(&m)
            .ok_or_else(|| CommandError::not_found(id)),
        _ if doc.contains(id) => Err(CommandError::wrong_kind(id, "mesh")),
        _ => Err(CommandError::not_found(id)),
    }
}

/// Mesh validation errors as one message; warnings are returned separately.
pub fn mesh_problems(id: ObjectId, mesh: &editor_core::Mesh) -> Result<Vec<String>, CommandError> {
    use editor_geom::mesh::{Severity, validate};
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    for issue in validate(mesh) {
        let text = format!("{id}: {issue:?}");
        match issue.severity() {
            Severity::Error => errors.push(text),
            Severity::Warning => warnings.push(text),
        }
    }
    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(CommandError::invalid_geometry(
            id,
            errors.join("; "),
            "check the vertex indices and that every face has at least 3 distinct vertices",
        ))
    }
}
