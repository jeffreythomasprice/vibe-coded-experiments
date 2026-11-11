use std::collections::BTreeSet;

use editor_core::{Entity, EntityId, Extra, GroupId, LayerId, Object, ObjectId, Properties};
use glam::DVec3;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results;
use super::util::{RESERVED_PROPERTIES, check_finite, non_empty, resolve_layer, with_entity};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

fn check_classname(classname: &str) -> Result<(), CommandError> {
    if classname.trim().is_empty() {
        return Err(CommandError::invalid_arg(
            "classname",
            "classname must not be empty",
        ));
    }
    Ok(())
}

fn check_property_keys<'a>(keys: impl Iterator<Item = &'a String>) -> Result<(), CommandError> {
    for key in keys {
        if key.trim().is_empty() {
            return Err(CommandError::invalid_arg(
                "properties",
                "property names must not be empty",
            ));
        }
        if RESERVED_PROPERTIES.contains(&key.as_str()) {
            return Err(CommandError::invalid_arg(
                "properties",
                format!("`{key}` is not a property"),
            )
            .with_hint("use entity.set_classname or entity.set_transform"));
        }
    }
    Ok(())
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

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Create {
    classname: String,
    #[schemars(with = "[f64; 3]")]
    origin: DVec3,
    #[schemars(with = "Option<[f64; 3]>")]
    angles: Option<DVec3>,
    #[serde(default)]
    properties: Properties,
    layer: Option<LayerId>,
    group: Option<GroupId>,
}

pub fn create(ctx: &mut Ctx, args: Create) -> CmdResult<results::Created> {
    check_classname(&args.classname)?;
    check_finite("origin", args.origin)?;
    let angles = args.angles.unwrap_or(DVec3::ZERO);
    check_angles(angles)?;
    check_property_keys(args.properties.keys())?;

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
    ctx.insert(Object::Entity(Entity {
        flags: Default::default(),
        id,
        classname: args.classname,
        layer,
        group,
        origin: Some(args.origin),
        angles,
        properties: args.properties,
        extra: Extra::new(),
    }));
    Ok(Outcome::created(vec![id.into()], vec![]))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateBrushEntity {
    classname: String,
    objects: Vec<ObjectId>,
    #[serde(default)]
    properties: Properties,
}

pub fn create_brush_entity(ctx: &mut Ctx, args: CreateBrushEntity) -> CmdResult<results::Created> {
    check_classname(&args.classname)?;
    check_property_keys(args.properties.keys())?;
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

    let id = EntityId(ctx.alloc_id());
    ctx.insert(Object::Entity(Entity {
        flags: Default::default(),
        id,
        classname: args.classname,
        layer,
        group,
        origin: None,
        angles: DVec3::ZERO,
        properties: args.properties,
        extra: Extra::new(),
    }));
    for member in members {
        ctx.insert(with_entity(member, Some(id)));
    }
    Ok(Outcome::created(vec![id.into()], vec![]))
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
                let entity = entity_of(ctx, id)?;
                if entity.origin.is_some() {
                    return Err(CommandError::wrong_kind(id, "brush entity"));
                }
                members.extend(ctx.doc().children(id));
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

    let owners: BTreeSet<EntityId> = members
        .iter()
        .filter_map(|&id| ctx.doc().get(id).and_then(|o| o.entity()))
        .collect();
    for &id in &members {
        if let Some(object) = ctx.doc().get(id) {
            ctx.insert(with_entity(object, None));
        }
    }
    let mut warnings = Vec::new();
    let mut removed = 0;
    for owner in owners {
        if ctx.doc().children(owner.into()).is_empty() {
            ctx.remove(owner.into());
            removed += 1;
            warnings.push(format!("{owner} has no objects left and was deleted"));
        }
    }
    Ok(Outcome {
        result: results::Released {
            released: members.len(),
            deleted_entities: removed,
        },
        warnings,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetProperties {
    pub ids: Vec<ObjectId>,
    #[serde(default)]
    pub set: Properties,
    #[serde(default)]
    pub remove: Vec<String>,
}

pub fn set_properties(ctx: &mut Ctx, args: SetProperties) -> CmdResult<results::Updated> {
    non_empty("ids", &args.ids)?;
    check_property_keys(args.set.keys().chain(args.remove.iter()))?;
    let mut updated = Vec::new();
    for &id in &args.ids {
        let mut entity = entity_of(ctx, id)?;
        for (k, v) in &args.set {
            entity.properties.insert(k.clone(), v.clone());
        }
        for k in &args.remove {
            entity.properties.remove(k);
        }
        updated.push(entity);
    }
    for entity in updated {
        ctx.insert(Object::Entity(entity));
    }
    Ok(Outcome::new(results::Updated {
        updated: args.ids.len(),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetClassname {
    pub ids: Vec<ObjectId>,
    pub classname: String,
}

pub fn set_classname(ctx: &mut Ctx, args: SetClassname) -> CmdResult<results::Updated> {
    non_empty("ids", &args.ids)?;
    check_classname(&args.classname)?;
    let mut updated = Vec::new();
    for &id in &args.ids {
        let mut entity = entity_of(ctx, id)?;
        entity.classname = args.classname.clone();
        updated.push(entity);
    }
    for entity in updated {
        ctx.insert(Object::Entity(entity));
    }
    Ok(Outcome::new(results::Updated {
        updated: args.ids.len(),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTransform {
    pub id: ObjectId,
    #[schemars(with = "Option<[f64; 3]>")]
    pub origin: Option<DVec3>,
    #[schemars(with = "Option<[f64; 3]>")]
    pub angles: Option<DVec3>,
}

pub fn set_transform(ctx: &mut Ctx, args: SetTransform) -> CmdResult<results::IdOnly> {
    let mut entity = entity_of(ctx, args.id)?;
    if entity.origin.is_none() {
        return Err(CommandError::invalid_arg(
            "id",
            format!("{} is a brush entity and has no origin", args.id),
        )
        .with_hint("move its brushes with transform.translate"));
    }
    if let Some(origin) = args.origin {
        check_finite("origin", origin)?;
        entity.origin = Some(origin);
    }
    if let Some(angles) = args.angles {
        check_angles(angles)?;
        entity.angles = angles;
    }
    ctx.insert(Object::Entity(entity));
    Ok(Outcome::new(results::IdOnly { id: args.id }))
}
