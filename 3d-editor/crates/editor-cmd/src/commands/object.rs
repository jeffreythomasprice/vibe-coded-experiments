use std::collections::{BTreeMap, BTreeSet};

use editor_core::{EntityId, GroupId, LayerId, Object, ObjectId, Value};
use glam::{DAffine3, DVec3};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results;
use super::transform::{AxisArg, about, reflection, transform_object};
use super::util::{check_finite, descendants, non_empty, renumber, with_layer, world_object};
use super::{CmdResult, Outcome};
use crate::component_defs::Catalog;
use crate::ctx::Ctx;
use crate::error::CommandError;
use crate::links;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    pub ids: Vec<ObjectId>,
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

pub fn delete(ctx: &mut Ctx, args: Delete) -> CmdResult<results::Deleted> {
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

    let catalog = Catalog::resolve(ctx.doc(), ctx.scene_dir());
    let warnings: Vec<String> = links::doc_links(ctx.doc(), &catalog)
        .into_iter()
        .filter(|l| {
            l.to.is_some_and(|t| doomed.contains(&t.into())) && !doomed.contains(&l.from.into())
        })
        .map(|l| {
            format!(
                "{} still links to deleted {} (properties{})",
                l.from, l.value, l.pointer
            )
        })
        .collect();
    for &id in &doomed {
        ctx.remove(id);
    }
    Ok(Outcome::with_warnings(
        results::Deleted {
            deleted: doomed.len(),
        },
        warnings,
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Duplicate {
    ids: Vec<ObjectId>,
    #[schemars(with = "Option<[f64; 3]>")]
    offset: Option<DVec3>,
}

pub(super) fn remap(
    object: Object,
    new_id: ObjectId,
    map: &BTreeMap<ObjectId, ObjectId>,
) -> Object {
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

/// Points links between copied entities at the copies; links to anything not copied stay.
pub(super) fn remap_links(
    catalog: &Catalog,
    entity: &mut editor_core::Entity,
    map: &BTreeMap<ObjectId, ObjectId>,
) {
    for (name, data) in &mut entity.components {
        for link in catalog.links(name, data) {
            let Some(new) = link
                .value
                .parse::<ObjectId>()
                .ok()
                .and_then(|old| map.get(&old))
            else {
                continue;
            };
            let mut root = Value::Object(std::mem::take(data));
            if let Some(slot) = root.pointer_mut(&link.pointer) {
                *slot = Value::String(new.to_string());
            }
            if let Value::Object(props) = root {
                *data = props;
            }
        }
    }
}

fn copy_order(ctx: &Ctx, ids: &[ObjectId]) -> Result<Vec<ObjectId>, CommandError> {
    non_empty("ids", ids)?;
    let mut all = BTreeSet::new();
    for &id in ids {
        world_object(ctx.doc(), id)?;
        descendants(ctx.doc(), id, &mut all);
    }
    let mut order: Vec<ObjectId> = Vec::new();
    for &id in ids {
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
    Ok(order)
}

pub(super) enum Source<'a> {
    Ctx,
    Doc(&'a editor_core::Document),
}

/// Transformed deep copies of `order` with fresh ids; nothing is inserted.
fn make_copy(
    ctx: &mut Ctx,
    order: &[ObjectId],
    m: Option<&DAffine3>,
) -> Result<(Vec<Object>, Vec<ObjectId>), CommandError> {
    make_copy_from(ctx, Source::Ctx, order, m)
}

pub(super) fn make_copy_from(
    ctx: &mut Ctx,
    source: Source,
    order: &[ObjectId],
    m: Option<&DAffine3>,
) -> Result<(Vec<Object>, Vec<ObjectId>), CommandError> {
    let map: BTreeMap<ObjectId, ObjectId> = order
        .iter()
        .map(|&old| (old, renumber(old, ctx.alloc_id())))
        .collect();
    let catalog = Catalog::resolve(ctx.doc(), ctx.scene_dir());
    let mut copies = Vec::new();
    for &old in order {
        let original = match &source {
            Source::Ctx => ctx.doc().get(old),
            Source::Doc(doc) => doc.get(old),
        }
        .expect("collected from the source document");
        let mut copy = remap(original, map[&old], &map);
        if let Object::Entity(e) = &mut copy {
            remap_links(&catalog, e, &map);
        }
        if let Some(m) = m {
            copy = transform_object(&copy, m, true)?;
        }
        copies.push(copy);
    }
    Ok((copies, order.iter().map(|id| map[id]).collect()))
}

pub fn duplicate(ctx: &mut Ctx, args: Duplicate) -> CmdResult<results::Duplicated> {
    let offset = args.offset.unwrap_or(DVec3::ZERO);
    check_finite("offset", offset)?;
    let order = copy_order(ctx, &args.ids)?;
    let shift = DAffine3::from_translation(offset);
    let (copies, ids) = make_copy(ctx, &order, (offset != DVec3::ZERO).then_some(&shift))?;
    for copy in copies {
        ctx.insert(copy);
    }
    let map = order.iter().copied().zip(ids.iter().copied()).collect();
    Ok(Outcome::new(results::Duplicated {
        id: ids.first().copied(),
        ids,
        map,
    }))
}

const MAX_ARRAY_COUNT: u32 = 256;
const MAX_ARRAY_OBJECTS: usize = 10_000;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Mirror {
    axis: AxisArg,
    #[schemars(with = "[f64; 3]")]
    pivot: DVec3,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Array {
    ids: Vec<ObjectId>,
    /// Extra copies at 1..=count times the offset (1 to 256).
    count: u32,
    #[schemars(with = "[f64; 3]")]
    offset: DVec3,
    /// Also add a mirrored copy of the whole row (originals included) across this plane.
    mirror: Option<Mirror>,
}

pub fn array(ctx: &mut Ctx, args: Array) -> CmdResult<results::Arrayed> {
    if args.count == 0 || args.count > MAX_ARRAY_COUNT {
        return Err(CommandError::invalid_arg(
            "count",
            format!("count must be between 1 and {MAX_ARRAY_COUNT}"),
        ));
    }
    check_finite("offset", args.offset)?;
    if args.offset == DVec3::ZERO {
        return Err(CommandError::invalid_arg(
            "offset",
            "offset must not be zero",
        ));
    }
    let mirror = match &args.mirror {
        Some(m) => {
            check_finite("mirror.pivot", m.pivot)?;
            let n = m.axis.unit("mirror.axis")?;
            Some(about(m.pivot, reflection(n)))
        }
        None => None,
    };
    let order = copy_order(ctx, &args.ids)?;
    let rows = args.count as usize + 1;
    let total = order.len() * (args.count as usize + if mirror.is_some() { rows } else { 0 });
    if total > MAX_ARRAY_OBJECTS {
        return Err(CommandError::invalid_arg(
            "count",
            format!("this would create {total} objects; the limit is {MAX_ARRAY_OBJECTS}"),
        ));
    }

    let mut inserts = Vec::new();
    let mut ids = Vec::new();
    let mut map: BTreeMap<ObjectId, Vec<ObjectId>> = BTreeMap::new();
    let mut place = |ctx: &mut Ctx, k: usize, mirrored: bool| -> Result<(), CommandError> {
        let shift = DAffine3::from_translation(args.offset * k as f64);
        let m = match (&mirror, mirrored) {
            (Some(r), true) => *r * shift,
            _ => shift,
        };
        let identity = k == 0 && !mirrored;
        if identity {
            return Ok(());
        }
        let (copies, new_ids) = make_copy(ctx, &order, Some(&m))?;
        for (old, new) in order.iter().zip(&new_ids) {
            map.entry(*old).or_default().push(*new);
        }
        inserts.extend(copies);
        ids.extend(new_ids);
        Ok(())
    };
    for k in 1..rows {
        place(ctx, k, false)?;
    }
    if mirror.is_some() {
        for k in 0..rows {
            place(ctx, k, true)?;
        }
    }
    for copy in inserts {
        ctx.insert(copy);
    }
    Ok(Outcome::new(results::Arrayed {
        id: ids.first().copied(),
        ids,
        map,
        copy_size: order.len(),
    }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Rename {
    pub id: ObjectId,
    pub name: String,
}

pub fn rename(ctx: &mut Ctx, args: Rename) -> CmdResult<results::IdOnly> {
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
    Ok(Outcome::new(results::IdOnly { id: args.id }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetLayer {
    pub ids: Vec<ObjectId>,
    pub layer: LayerId,
}

pub fn set_layer(ctx: &mut Ctx, args: SetLayer) -> CmdResult<results::Moved> {
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
    Ok(Outcome::new(results::Moved { moved: count }))
}
