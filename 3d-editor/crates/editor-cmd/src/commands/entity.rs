use std::collections::{BTreeMap, BTreeSet};

use editor_core::{
    Entity, EntityId, Extra, GroupId, LayerId, Object, ObjectId, Properties, find_null,
    valid_component_name,
};
use editor_geom::euler::orientation;
use glam::{DAffine3, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results;
use super::util::{check_finite, non_empty, resolve_layer, with_entity};
use super::{CmdResult, Outcome};
use crate::component_defs::Catalog;
use crate::ctx::Ctx;
use crate::error::CommandError;

pub type Components = BTreeMap<String, Properties>;

fn check_name(name: &str) -> Result<(), CommandError> {
    if valid_component_name(name) {
        Ok(())
    } else {
        Err(CommandError::invalid_arg(
            "name",
            format!("`{name}` is not a component name: use letters, digits and underscores"),
        ))
    }
}

fn check_components(field: &str, components: &Components) -> Result<(), CommandError> {
    for (name, data) in components {
        check_name(name)?;
        check_data(field, name, data)?;
    }
    Ok(())
}

fn check_data(field: &str, name: &str, data: &Properties) -> Result<(), CommandError> {
    for (key, value) in data {
        if let Some(pointer) = find_null(value) {
            return Err(CommandError::invalid_arg(
                field,
                format!("`{name}/{key}{pointer}` is null, which scenes cannot store"),
            )
            .with_hint("remove the key instead, or use false, 0 or an empty string"));
        }
    }
    Ok(())
}

fn warnings_for(catalog: &Catalog, id: EntityId, components: &Components) -> Vec<String> {
    components
        .iter()
        .flat_map(|(name, data)| {
            catalog
                .component_issues(name, data)
                .into_iter()
                .map(move |issue| format!("{id}: {name}: {issue}"))
        })
        .collect()
}

fn check_angles(angles: DVec3) -> Result<(), CommandError> {
    if angles.is_finite() {
        Ok(())
    } else {
        Err(CommandError::invalid_arg(
            "angles",
            "angles must be finite numbers",
        ))
    }
}

fn entity_of(ctx: &Ctx, id: ObjectId) -> Result<Entity, CommandError> {
    match id {
        ObjectId::Entity(e) => ctx
            .doc()
            .entities
            .get(&e)
            .cloned()
            .ok_or_else(|| CommandError::not_found(id)),
        _ if ctx.doc().contains(id) => Err(CommandError::wrong_kind(id, "entity")),
        _ => Err(CommandError::not_found(id)),
    }
}

fn pose(origin: DVec3, angles: DVec3) -> DAffine3 {
    DAffine3::from_translation(origin) * DAffine3::from_mat3(orientation(angles))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Create {
    #[schemars(with = "[f64; 3]")]
    pub origin: DVec3,
    #[schemars(with = "Option<[f64; 3]>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angles: Option<DVec3>,
    /// Component name to its data object. Data is not filled from schema defaults here; use entity.add_component for that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<LayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<GroupId>,
}

pub fn create(ctx: &mut Ctx, args: Create) -> CmdResult<results::Created> {
    check_finite("origin", args.origin)?;
    let angles = args.angles.unwrap_or(DVec3::ZERO);
    check_angles(angles)?;
    let components = args.components.unwrap_or_default();
    check_components("components", &components)?;

    let (layer, group) = match args.group {
        Some(g) => {
            let group = ctx
                .doc()
                .groups
                .get(&g)
                .ok_or_else(|| CommandError::not_found(g))?;
            if args.layer.is_some_and(|l| l != group.layer) {
                return Err(CommandError::invalid_arg(
                    "layer",
                    format!("{g} is on layer {}", group.layer),
                ));
            }
            (group.layer, Some(g))
        }
        None => (resolve_layer(ctx.doc(), args.layer)?, None),
    };
    let id = EntityId(ctx.alloc_id());
    let warnings = warnings_for(
        &Catalog::resolve(ctx.doc(), ctx.scene_dir()),
        id,
        &components,
    );
    ctx.insert(Object::Entity(Entity {
        flags: Default::default(),
        id,
        layer,
        group,
        origin: args.origin,
        angles,
        components,
        extra: Extra::new(),
    }));
    Ok(Outcome::created(vec![id.into()], warnings))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateBrushEntity {
    pub objects: Vec<ObjectId>,
    /// The pivot. Default: the centre of the members' bounds.
    #[schemars(with = "Option<[f64; 3]>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<DVec3>,
    #[schemars(with = "Option<[f64; 3]>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angles: Option<DVec3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Components>,
}

pub fn create_brush_entity(ctx: &mut Ctx, args: CreateBrushEntity) -> CmdResult<results::Created> {
    let components = args.components.unwrap_or_default();
    check_components("components", &components)?;
    let angles = args.angles.unwrap_or(DVec3::ZERO);
    check_angles(angles)?;
    non_empty("objects", &args.objects)?;

    let mut members = Vec::new();
    for &id in &args.objects {
        let object = super::util::brush_or_mesh(ctx.doc(), id)?;
        if let Some(owner) = object.entity() {
            return Err(CommandError::invalid_arg(
                "objects",
                format!("{id} already belongs to {owner}"),
            )
            .with_hint("call entity.release_objects first"));
        }
        members.push(object);
    }
    let (layer, group) = (
        members[0].layer().expect("brushes have layers"),
        members[0].group(),
    );
    if let Some(odd) = members
        .iter()
        .find(|m| m.layer() != Some(layer) || m.group() != group)
    {
        return Err(CommandError::invalid_arg(
            "objects",
            format!(
                "{} is not on the same layer and group as {}",
                odd.id(),
                members[0].id()
            ),
        )
        .with_hint("use object.set_layer or group.move to line them up"));
    }
    let origin = match args.origin {
        Some(o) => {
            check_finite("origin", o)?;
            o
        }
        None => members
            .iter()
            .filter_map(crate::bounds::object_bounds)
            .reduce(|a, b| a.merge(b))
            .map(|b| b.center())
            .unwrap_or(DVec3::ZERO),
    };

    let id = EntityId(ctx.alloc_id());
    let warnings = warnings_for(
        &Catalog::resolve(ctx.doc(), ctx.scene_dir()),
        id,
        &components,
    );
    ctx.insert(Object::Entity(Entity {
        flags: Default::default(),
        id,
        layer,
        group,
        origin,
        angles,
        components,
        extra: Extra::new(),
    }));
    for member in members {
        ctx.insert(with_entity(member, Some(id)));
    }
    Ok(Outcome::created(vec![id.into()], warnings))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Release {
    ids: Vec<ObjectId>,
}

pub fn release_objects(ctx: &mut Ctx, args: Release) -> CmdResult<results::Released> {
    non_empty("ids", &args.ids)?;
    let mut members = BTreeSet::new();
    for &id in &args.ids {
        match id {
            ObjectId::Entity(_) => {
                entity_of(ctx, id)?;
                let children = ctx.doc().children(id);
                if children.is_empty() {
                    return Err(CommandError::invalid_arg(
                        "ids",
                        format!("{id} has no brushes or meshes to release"),
                    ));
                }
                members.extend(children);
            }
            _ => {
                let object = super::util::brush_or_mesh(ctx.doc(), id)?;
                if object.entity().is_none() {
                    return Err(CommandError::invalid_arg(
                        "ids",
                        format!("{id} does not belong to an entity"),
                    ));
                }
                members.insert(id);
            }
        }
    }
    for &id in &members {
        if let Some(object) = ctx.doc().get(id) {
            ctx.insert(with_entity(object, None));
        }
    }
    Ok(Outcome::new(results::Released {
        released: members.len(),
    }))
}

fn resolved_ids(ctx: &Ctx, ids: &[ObjectId]) -> Result<Vec<Entity>, CommandError> {
    non_empty("ids", ids)?;
    ids.iter().map(|&id| entity_of(ctx, id)).collect()
}

fn store(ctx: &mut Ctx, name: &str, entities: Vec<Entity>) -> CmdResult<results::Updated> {
    let catalog = Catalog::resolve(ctx.doc(), ctx.scene_dir());
    let count = entities.len();
    let mut warnings = Vec::new();
    for entity in entities {
        if let Some(data) = entity.components.get(name) {
            warnings.extend(
                catalog
                    .component_issues(name, data)
                    .into_iter()
                    .map(|issue| format!("{}: {name}: {issue}", entity.id)),
            );
        }
        ctx.insert(Object::Entity(entity));
    }
    Ok(Outcome::with_warnings(
        results::Updated { updated: count },
        warnings,
    ))
}

fn missing(id: EntityId, name: &str) -> CommandError {
    CommandError::invalid_arg("name", format!("{id} has no component `{name}`"))
        .with_hint("add it with entity.add_component")
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AddComponent {
    pub ids: Vec<ObjectId>,
    pub name: String,
    /// JSON object. Omitted: the schema's top-level property defaults, when the component has a definition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Properties>,
}

pub fn add_component(ctx: &mut Ctx, args: AddComponent) -> CmdResult<results::Updated> {
    check_name(&args.name)?;
    if let Some(data) = &args.data {
        check_data("data", &args.name, data)?;
    }
    let data = match args.data {
        Some(d) => d,
        None => Catalog::resolve(ctx.doc(), ctx.scene_dir()).defaults(&args.name),
    };
    let mut entities = resolved_ids(ctx, &args.ids)?;
    for entity in &mut entities {
        if entity.components.contains_key(&args.name) {
            return Err(CommandError::invalid_arg(
                "name",
                format!("{} already has component `{}`", entity.id, args.name),
            )
            .with_hint("use entity.set_component or entity.update_component"));
        }
        entity.components.insert(args.name.clone(), data.clone());
    }
    store(ctx, &args.name, entities)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RemoveComponent {
    pub ids: Vec<ObjectId>,
    pub name: String,
}

pub fn remove_component(ctx: &mut Ctx, args: RemoveComponent) -> CmdResult<results::Updated> {
    check_name(&args.name)?;
    let mut entities = resolved_ids(ctx, &args.ids)?;
    for entity in &mut entities {
        if entity.components.remove(&args.name).is_none() {
            return Err(missing(entity.id, &args.name));
        }
    }
    store(ctx, &args.name, entities)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetComponent {
    pub ids: Vec<ObjectId>,
    pub name: String,
    /// The whole new data object. Its shape is defined by the component schema; the editor only warns when it does not match.
    pub data: Properties,
}

pub fn set_component(ctx: &mut Ctx, args: SetComponent) -> CmdResult<results::Updated> {
    check_name(&args.name)?;
    check_data("data", &args.name, &args.data)?;
    let mut entities = resolved_ids(ctx, &args.ids)?;
    for entity in &mut entities {
        let Some(slot) = entity.components.get_mut(&args.name) else {
            return Err(missing(entity.id, &args.name));
        };
        *slot = args.data.clone();
    }
    store(ctx, &args.name, entities)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpdateComponent {
    pub ids: Vec<ObjectId>,
    pub name: String,
    /// Top-level keys to set.
    #[serde(default)]
    pub set: Properties,
    /// Top-level keys to remove.
    #[serde(default)]
    pub remove: Vec<String>,
}

pub fn update_component(ctx: &mut Ctx, args: UpdateComponent) -> CmdResult<results::Updated> {
    check_name(&args.name)?;
    check_data("set", &args.name, &args.set)?;
    let mut entities = resolved_ids(ctx, &args.ids)?;
    for entity in &mut entities {
        let Some(data) = entity.components.get_mut(&args.name) else {
            return Err(missing(entity.id, &args.name));
        };
        for (k, v) in &args.set {
            data.insert(k.clone(), v.clone());
        }
        for k in &args.remove {
            data.remove(k);
        }
    }
    store(ctx, &args.name, entities)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Links {
    /// Entities whose outgoing links to list; default all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<ObjectId>>,
}

pub fn links(ctx: &Ctx, args: Links) -> CmdResult<results::Links> {
    let filter: Option<BTreeSet<EntityId>> = match args.ids {
        None => None,
        Some(ids) => {
            let mut set = BTreeSet::new();
            for id in ids {
                match id {
                    ObjectId::Entity(e) if ctx.doc().entities.contains_key(&e) => {
                        set.insert(e);
                    }
                    _ if ctx.doc().contains(id) => {
                        return Err(CommandError::wrong_kind(id, "entity"));
                    }
                    _ => return Err(CommandError::not_found(id)),
                }
            }
            Some(set)
        }
    };
    let catalog = Catalog::resolve(ctx.doc(), ctx.scene_dir());
    let mut links = crate::links::doc_links(ctx.doc(), &catalog);
    if let Some(filter) = filter {
        links.retain(|l| filter.contains(&l.from));
    }
    Ok(Outcome::new(results::Links { links }))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Members {
    /// Move the member brushes and meshes by the same rigid change (default).
    #[default]
    Carry,
    /// Move only the pivot.
    Keep,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTransform {
    pub id: ObjectId,
    #[schemars(with = "Option<[f64; 3]>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<DVec3>,
    #[schemars(with = "Option<[f64; 3]>")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angles: Option<DVec3>,
    /// For a brush entity: carry the members along with the pivot, or move only the pivot.
    #[serde(default)]
    pub members: Members,
}

pub fn set_transform(ctx: &mut Ctx, args: SetTransform) -> CmdResult<results::IdOnly> {
    let mut entity = entity_of(ctx, args.id)?;
    let before = pose(entity.origin, entity.angles);
    if let Some(origin) = args.origin {
        check_finite("origin", origin)?;
        entity.origin = origin;
    }
    if let Some(angles) = args.angles {
        check_angles(angles)?;
        entity.angles = angles;
    }
    let delta = pose(entity.origin, entity.angles) * before.inverse();
    ctx.insert(Object::Entity(entity));
    if args.members == Members::Carry {
        for child in ctx.doc().children(args.id) {
            if let Some(object) = ctx.doc().get(child) {
                let moved = super::transform::transform_object(&object, &delta, true)?;
                ctx.insert(moved);
            }
        }
    }
    Ok(Outcome::new(results::IdOnly { id: args.id }))
}
