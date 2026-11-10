use std::collections::{BTreeMap, BTreeSet};

use glam::{DAffine3, DVec3};
use level_core::{EntityId, GroupId, LayerId, Object, ObjectId};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::json;

use super::transform::transform_object;
use super::util::{check_finite, descendants, non_empty, renumber, with_layer, world_object};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    ids: Vec<ObjectId>,
}

fn collect(ctx: &Ctx, id: ObjectId, out: &mut BTreeSet<ObjectId>) {
    if !out.insert(id) {
        return;
    }
    if matches!(id, ObjectId::Entity(_) | ObjectId::Group(_)) {
        for child in ctx.doc().children(id) {
            collect(ctx, child, out);
        }
    }
}

pub fn delete(ctx: &mut Ctx, args: Delete) -> CmdResult {
    if args.ids.is_empty() {
        return Err(CommandError::invalid_arg("ids", "ids must not be empty"));
    }
    let mut doomed = BTreeSet::new();
    for &id in &args.ids {
        if !ctx.doc().contains(id) {
            return Err(CommandError::not_found(id));
        }
        collect(ctx, id, &mut doomed);
    }

    let layers: Vec<_> = doomed
        .iter()
        .filter(|id| matches!(id, ObjectId::Layer(_)))
        .copied()
        .collect();
    for &layer in &layers {
        if let Some(child) = ctx
            .doc()
            .children(layer)
            .into_iter()
            .find(|c| !doomed.contains(c))
        {
            return Err(CommandError::invalid_arg(
                "ids",
                format!("{layer} still contains {child}"),
            )
            .with_hint("delete or move the layer's objects first"));
        }
    }
    if layers.len() >= ctx.doc().layers.len() {
        return Err(
            CommandError::invalid_arg("ids", "a document needs at least one layer")
                .with_hint("keep one layer, or create another before deleting this one"),
        );
    }

    for &id in &doomed {
        ctx.remove(id);
    }
    Ok(Outcome::new(json!({ "deleted": doomed.len() })))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Duplicate {
    ids: Vec<ObjectId>,
    #[schemars(with = "Option<[f64; 3]>")]
    offset: Option<DVec3>,
}

fn remap(object: Object, new_id: ObjectId, map: &BTreeMap<ObjectId, ObjectId>) -> Object {
    let group = |g: Option<GroupId>| match g.and_then(|g| map.get(&g.into())) {
        Some(ObjectId::Group(n)) => Some(*n),
        _ => g,
    };
    let entity = |e: Option<EntityId>| match e.and_then(|e| map.get(&e.into())) {
        Some(ObjectId::Entity(n)) => Some(*n),
        _ => e,
    };
    match (object, new_id) {
        (Object::Brush(mut o), ObjectId::Brush(id)) => {
            o.id = id;
            o.group = group(o.group);
            o.entity = entity(o.entity);
            Object::Brush(o)
        }
        (Object::Mesh(mut o), ObjectId::Mesh(id)) => {
            o.id = id;
            o.group = group(o.group);
            o.entity = entity(o.entity);
            Object::Mesh(o)
        }
        (Object::Entity(mut o), ObjectId::Entity(id)) => {
            o.id = id;
            o.group = group(o.group);
            Object::Entity(o)
        }
        (Object::Group(mut o), ObjectId::Group(id)) => {
            o.id = id;
            o.parent = group(o.parent);
            Object::Group(o)
        }
        (other, _) => other,
    }
}

pub fn duplicate(ctx: &mut Ctx, args: Duplicate) -> CmdResult {
    non_empty("ids", &args.ids)?;
    let offset = args.offset.unwrap_or(DVec3::ZERO);
    check_finite("offset", offset)?;

    let mut all = BTreeSet::new();
    for &id in &args.ids {
        world_object(ctx.doc(), id)?;
        descendants(ctx.doc(), id, &mut all);
    }
    let mut order: Vec<ObjectId> = Vec::new();
    for &id in &args.ids {
        if !order.contains(&id) {
            order.push(id);
        }
    }
    let rest: Vec<ObjectId> = all
        .iter()
        .filter(|id| !order.contains(id))
        .copied()
        .collect();
    order.extend(rest);

    let map: BTreeMap<ObjectId, ObjectId> = order
        .iter()
        .map(|&old| (old, renumber(old, ctx.alloc_id())))
        .collect();

    let shift = DAffine3::from_translation(offset);
    let mut copies = Vec::new();
    for &old in &order {
        let source = ctx.doc().get(old).expect("collected from the document");
        let mut copy = remap(source, map[&old], &map);
        if offset != DVec3::ZERO {
            copy = transform_object(&copy, &shift)?;
        }
        copies.push(copy);
    }
    for copy in copies {
        ctx.insert(copy);
    }

    let ids: Vec<ObjectId> = order.iter().map(|id| map[id]).collect();
    let mut outcome = Outcome::created(ids, vec![]);
    outcome.result["map"] = json!(
        map.iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect::<BTreeMap<_, _>>()
    );
    Ok(outcome)
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    id: ObjectId,
    name: String,
}

pub fn rename(ctx: &mut Ctx, args: Rename) -> CmdResult {
    let object = ctx
        .doc()
        .get(args.id)
        .ok_or_else(|| CommandError::not_found(args.id))?;
    if args.name.trim().is_empty() {
        return Err(CommandError::invalid_arg("name", "name must not be empty"));
    }
    let renamed = match object {
        Object::Group(mut g) => {
            g.name = args.name;
            Object::Group(g)
        }
        Object::Layer(mut l) => {
            super::layer::check_layer_name(ctx, &args.name, Some(l.id))?;
            l.name = args.name;
            Object::Layer(l)
        }
        Object::Camera(mut c) => {
            super::view::check_name(ctx.doc(), &args.name, Some(c.id))?;
            c.name = args.name;
            Object::Camera(c)
        }
        _ => return Err(CommandError::wrong_kind(args.id, "group, layer or camera")),
    };
    ctx.insert(renamed);
    Ok(Outcome::new(json!({ "id": args.id })))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetLayer {
    ids: Vec<ObjectId>,
    layer: LayerId,
}

pub fn set_layer(ctx: &mut Ctx, args: SetLayer) -> CmdResult {
    non_empty("ids", &args.ids)?;
    if !ctx.doc().layers.contains_key(&args.layer) {
        return Err(CommandError::not_found(args.layer));
    }
    let mut moving = BTreeSet::new();
    for &id in &args.ids {
        world_object(ctx.doc(), id)?;
        descendants(ctx.doc(), id, &mut moving);
    }
    for &id in &moving {
        let object = ctx.doc().get(id).expect("collected from the document");
        let outside_parent = [
            object.entity().map(ObjectId::from),
            object.group().map(ObjectId::from),
        ]
        .into_iter()
        .flatten()
        .find(|p| !moving.contains(p));
        if let Some(p) = outside_parent
            && ctx.doc().get(p).and_then(|o| o.layer()) != Some(args.layer)
        {
            return Err(CommandError::invalid_arg(
                "ids",
                format!("{id} belongs to {p}, which stays on another layer"),
            )
            .with_hint("move the parent too, or release the object first (group.move, entity.release_objects)"));
        }
    }
    let mut count = 0;
    for id in moving {
        let object = ctx.doc().get(id).expect("collected from the document");
        if object.layer() != Some(args.layer) {
            ctx.insert(with_layer(object, args.layer));
            count += 1;
        }
    }
    Ok(Outcome::new(json!({ "moved": count })))
}
