use std::collections::{BTreeMap, BTreeSet};

use glam::DVec3;
use level_core::{ElementRef, EntityId, GroupId, LayerId, Object, ObjectId, ObjectKind, Part};
use level_geom::ray::{RayHit, ray_aabb, ray_brush, ray_mesh};
use level_geom::{Aabb, brush_geometry, brush_intersect};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};

use super::util::{BoxArg, brush_or_mesh, check_finite, non_empty};
use super::{CmdResult, NoArgs, Outcome};
use crate::bounds::{bounds_json, object_bounds, scene_bounds};
use crate::ctx::Ctx;
use crate::error::CommandError;

pub fn summary(ctx: &Ctx, _: NoArgs) -> CmdResult {
    let doc = ctx.doc();
    let bounds = scene_bounds(doc);

    let mut classnames: BTreeMap<&str, usize> = BTreeMap::new();
    for e in doc.entities.values() {
        *classnames.entry(e.classname.as_str()).or_default() += 1;
    }
    let layers: Vec<Value> = doc
        .layers
        .values()
        .map(|l| {
            json!({
                "id": l.id,
                "name": l.name,
                "objects": doc.children(l.id.into()).len(),
            })
        })
        .collect();

    Ok(Outcome::new(json!({
        "name": doc.name,
        "counts": {
            "brush": doc.brushes.len(),
            "mesh": doc.meshes.len(),
            "entity": doc.entities.len(),
            "group": doc.groups.len(),
            "layer": doc.layers.len(),
            "camera": doc.cameras.len(),
        },
        "bounds": bounds.map(bounds_json),
        "layers": layers,
        "classnames": classnames,
        "selection": ctx.selection().len(),
    })))
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    #[default]
    Summary,
    Full,
    Geometry,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Get {
    ids: Vec<ObjectId>,
    #[serde(default)]
    detail: Detail,
}

fn describe(object: &Object, detail: Detail) -> Value {
    let mut out = match detail {
        Detail::Summary => {
            let mut v = json!({ "id": object.id(), "kind": object.id().kind() });
            if let Some(l) = object.layer() {
                v["layer"] = json!(l);
            }
            if let Some(g) = object.group() {
                v["group"] = json!(g);
            }
            if let Some(e) = object.entity() {
                v["entity"] = json!(e);
            }
            match object {
                Object::Entity(e) => v["classname"] = json!(e.classname),
                Object::Group(g) => v["name"] = json!(g.name),
                Object::Layer(l) => v["name"] = json!(l.name),
                Object::Camera(c) => v["name"] = json!(c.name),
                Object::Brush(b) => v["faces"] = json!(b.brush.faces.len()),
                Object::Mesh(m) => {
                    v["vertices"] = json!(m.mesh.vertices.len());
                    v["faces"] = json!(m.mesh.faces.len());
                }
            }
            if let Some(b) = object_bounds(object) {
                v["bounds"] = bounds_json(b);
            }
            v
        }
        Detail::Full | Detail::Geometry => {
            let mut v = serde_json::to_value(object).expect("objects serialize");
            if let Some(b) = object_bounds(object) {
                v["bounds"] = bounds_json(b);
            }
            v
        }
    };

    if matches!(detail, Detail::Geometry) {
        let geometry = match object {
            Object::Brush(b) => Some(match brush_geometry(&b.brush) {
                Ok(g) => json!({
                    "vertices": g.vertices,
                    "faces": g.faces.iter().map(|f| json!({
                        "id": f.face.0,
                        "normal": f.plane.normal,
                        "dist": f.plane.dist,
                        "loop": f.loop_,
                    })).collect::<Vec<_>>(),
                }),
                Err(e) => json!({ "error": e.to_string() }),
            }),
            _ => None,
        };
        if let Some(g) = geometry {
            out["geometry"] = g;
        }
    }
    out
}

pub fn get(ctx: &Ctx, args: Get) -> CmdResult {
    let doc = ctx.doc();
    let objects = args
        .ids
        .iter()
        .map(|&id| {
            doc.get(id)
                .map(|o| describe(&o, args.detail))
                .ok_or_else(|| CommandError::not_found(id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Outcome::new(json!({ "objects": objects })))
}

const ENTITY_HALF: f64 = 8.0;
const OVERLAP_EPS: f64 = 1e-6;
const MAX_PAIRS: usize = 500;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct List {
    kind: Option<ObjectKind>,
    layer: Option<LayerId>,
    group: Option<GroupId>,
    entity: Option<EntityId>,
    classname: Option<String>,
    /// Exact name of a group, layer or camera.
    name: Option<String>,
    /// Only objects whose bounds lie inside this box.
    within: Option<BoxArg>,
    /// Only objects whose bounds touch this box.
    intersects: Option<BoxArg>,
    /// Keys to keep from each summary (`id` is always kept).
    fields: Option<Vec<String>>,
    /// Default 100, at most 1000.
    limit: Option<usize>,
    /// The `next_cursor` of the previous page.
    cursor: Option<ObjectId>,
}

fn encloses(outer: &Aabb, inner: &Aabb) -> bool {
    outer.min.cmple(inner.min).all() && inner.max.cmple(outer.max).all()
}

fn touches(a: &Aabb, b: &Aabb) -> bool {
    a.min.cmple(b.max).all() && b.min.cmple(a.max).all()
}

fn name_of(object: &Object) -> Option<&str> {
    match object {
        Object::Group(g) => Some(&g.name),
        Object::Layer(l) => Some(&l.name),
        Object::Camera(c) => Some(&c.name),
        _ => None,
    }
}

pub fn list(ctx: &Ctx, args: List) -> CmdResult {
    let limit = args.limit.unwrap_or(100);
    if !(1..=1000).contains(&limit) {
        return Err(CommandError::invalid_arg(
            "limit",
            "limit must be between 1 and 1000",
        ));
    }
    let within = args.within.map(|b| b.aabb("within")).transpose()?;
    let intersects = args.intersects.map(|b| b.aabb("intersects")).transpose()?;
    let doc = ctx.doc();

    let matches = |object: &Object| -> bool {
        if args.kind.is_some_and(|k| object.id().kind() != k) {
            return false;
        }
        if args.layer.is_some_and(|l| object.layer() != Some(l)) {
            return false;
        }
        if args.group.is_some_and(|g| object.group() != Some(g)) {
            return false;
        }
        if args.entity.is_some_and(|e| object.entity() != Some(e)) {
            return false;
        }
        if let Some(c) = &args.classname
            && !matches!(object, Object::Entity(e) if &e.classname == c)
        {
            return false;
        }
        if let Some(n) = &args.name
            && name_of(object) != Some(n.as_str())
        {
            return false;
        }
        if within.is_some() || intersects.is_some() {
            let Some(b) = object_bounds(object) else {
                return false;
            };
            if within.is_some_and(|w| !encloses(&w, &b)) {
                return false;
            }
            if intersects.is_some_and(|i| !touches(&i, &b)) {
                return false;
            }
        }
        true
    };

    let mut page: Vec<Object> = Vec::new();
    let mut more = false;
    for id in doc.all_ids() {
        if args.cursor.is_some_and(|c| id.number() <= c.number()) {
            continue;
        }
        let Some(object) = doc.get(id) else { continue };
        if !matches(&object) {
            continue;
        }
        if page.len() == limit {
            more = true;
            break;
        }
        page.push(object);
    }
    let next_cursor = more.then(|| page.last().map(Object::id)).flatten();
    let objects: Vec<Value> = page
        .iter()
        .map(|o| {
            let mut v = describe(o, Detail::Summary);
            if let (Some(fields), Some(map)) = (&args.fields, v.as_object_mut()) {
                map.retain(|k, _| k == "id" || fields.contains(k));
            }
            v
        })
        .collect();
    Ok(Outcome::new(
        json!({ "objects": objects, "next_cursor": next_cursor }),
    ))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    ids: Vec<ObjectId>,
}

pub fn bounds(ctx: &Ctx, args: Bounds) -> CmdResult {
    non_empty("ids", &args.ids)?;
    let mut combined: Option<Aabb> = None;
    let mut items = Vec::new();
    for &id in &args.ids {
        let object = ctx
            .doc()
            .get(id)
            .ok_or_else(|| CommandError::not_found(id))?;
        let b = object_bounds(&object);
        if let Some(b) = b {
            combined = Some(combined.map_or(b, |c| c.merge(b)));
        }
        items.push(json!({ "id": id, "bounds": b.map(bounds_json) }));
    }
    Ok(Outcome::new(
        json!({ "bounds": items, "combined": combined.map(bounds_json) }),
    ))
}

const SOLID_KINDS: [ObjectKind; 3] = [ObjectKind::Brush, ObjectKind::Mesh, ObjectKind::Entity];

fn check_kinds(kinds: &Option<Vec<ObjectKind>>) -> Result<BTreeSet<ObjectKind>, CommandError> {
    let kinds = kinds.clone().unwrap_or_else(|| SOLID_KINDS.to_vec());
    if let Some(bad) = kinds.iter().find(|k| !SOLID_KINDS.contains(k)) {
        return Err(CommandError::invalid_arg(
            "kinds",
            format!("{bad:?} has no extent; use brush, mesh or entity"),
        ));
    }
    Ok(kinds.into_iter().collect())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Raycast {
    #[schemars(with = "[f64; 3]")]
    origin: DVec3,
    #[schemars(with = "[f64; 3]")]
    dir: DVec3,
    max_dist: Option<f64>,
    /// brush, mesh and/or entity (default all three).
    kinds: Option<Vec<ObjectKind>>,
    #[serde(default)]
    ignore: Vec<ObjectId>,
}

pub fn raycast(ctx: &Ctx, args: Raycast) -> CmdResult {
    check_finite("origin", args.origin)?;
    let dir = args
        .dir
        .is_finite()
        .then(|| args.dir.try_normalize())
        .flatten()
        .ok_or_else(|| CommandError::invalid_arg("dir", "dir must be a non-zero finite vector"))?;
    if args.max_dist.is_some_and(|d| !(d.is_finite() && d > 0.0)) {
        return Err(CommandError::invalid_arg(
            "max_dist",
            "max_dist must be positive",
        ));
    }
    let kinds = check_kinds(&args.kinds)?;
    let ignore: BTreeSet<ObjectId> = args.ignore.iter().copied().collect();
    let doc = ctx.doc();
    let origin = args.origin;

    let mut best: Option<(ObjectId, RayHit)> = None;
    let mut consider = |id: ObjectId, hit: Option<RayHit>| {
        if let Some(h) = hit
            && args.max_dist.is_none_or(|m| h.t <= m)
            && best.is_none_or(|(_, b)| h.t < b.t)
        {
            best = Some((id, h));
        }
    };
    if kinds.contains(&ObjectKind::Brush) {
        for b in doc.brushes.values() {
            let id = ObjectId::from(b.id);
            if ignore.contains(&id) {
                continue;
            }
            let hit = brush_geometry(&b.brush)
                .ok()
                .and_then(|g| ray_brush(origin, dir, &g));
            consider(id, hit);
        }
    }
    if kinds.contains(&ObjectKind::Mesh) {
        for m in doc.meshes.values() {
            let id = ObjectId::from(m.id);
            if !ignore.contains(&id) {
                consider(id, ray_mesh(origin, dir, &m.mesh));
            }
        }
    }
    if kinds.contains(&ObjectKind::Entity) {
        for e in doc.entities.values() {
            let id = ObjectId::from(e.id);
            let Some(o) = e.origin else { continue };
            if ignore.contains(&id) {
                continue;
            }
            let bounds = Aabb {
                min: o - DVec3::splat(ENTITY_HALF),
                max: o + DVec3::splat(ENTITY_HALF),
            };
            consider(id, ray_aabb(origin, dir, &bounds));
        }
    }

    let hit = best.map(|(id, h)| {
        json!({
            "id": id,
            "face": h.face.map(|f| ElementRef { object: id, part: Part::Face(f) }.to_string()),
            "point": origin + dir * h.t,
            "normal": h.normal,
            "distance": h.t,
        })
    });
    Ok(Outcome::new(json!({ "hit": hit })))
}

#[derive(Debug, Clone, Copy, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum QueryMode {
    #[default]
    Intersects,
    Contains,
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct QueryBox {
    #[schemars(with = "[f64; 3]")]
    min: DVec3,
    #[schemars(with = "[f64; 3]")]
    max: DVec3,
    #[serde(default)]
    mode: QueryMode,
    kinds: Option<Vec<ObjectKind>>,
}

pub fn query_box(ctx: &Ctx, args: QueryBox) -> CmdResult {
    let region = BoxArg {
        min: args.min,
        max: args.max,
    }
    .aabb("min")?;
    let kinds = check_kinds(&args.kinds)?;
    let ids: Vec<ObjectId> = ctx
        .doc()
        .all_ids()
        .into_iter()
        .filter(|id| kinds.contains(&id.kind()))
        .filter(|&id| {
            ctx.doc()
                .get(id)
                .and_then(|o| object_bounds(&o))
                .is_some_and(|b| match args.mode {
                    QueryMode::Intersects => touches(&region, &b),
                    QueryMode::Contains => encloses(&region, &b),
                })
        })
        .collect();
    Ok(Outcome::new(json!({ "ids": ids, "count": ids.len() })))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Overlaps {
    /// Brushes and meshes to compare among themselves; default is every brush and mesh.
    ids: Option<Vec<ObjectId>>,
}

fn interiors_overlap(a: &Aabb, b: &Aabb) -> bool {
    let lo = a.min.max(b.min);
    let hi = a.max.min(b.max);
    (hi - lo).min_element() > OVERLAP_EPS
}

pub fn overlaps(ctx: &Ctx, args: Overlaps) -> CmdResult {
    let doc = ctx.doc();
    let ids: Vec<ObjectId> = match args.ids {
        Some(ids) => {
            non_empty("ids", &ids)?;
            for &id in &ids {
                brush_or_mesh(doc, id)?;
            }
            let mut ids = ids;
            ids.sort_by_key(|i| i.number());
            ids.dedup();
            ids
        }
        None => doc
            .brushes
            .keys()
            .map(|&b| ObjectId::from(b))
            .chain(doc.meshes.keys().map(|&m| ObjectId::from(m)))
            .collect(),
    };
    let items: Vec<(Object, Aabb)> = ids
        .iter()
        .filter_map(|&id| doc.get(id))
        .filter_map(|o| object_bounds(&o).map(|b| (o, b)))
        .collect();

    let mut pairs = Vec::new();
    let mut truncated = false;
    'outer: for (i, (a, ab)) in items.iter().enumerate() {
        for (b, bb) in &items[i + 1..] {
            if !interiors_overlap(ab, bb) {
                continue;
            }
            let approx = match (a, b) {
                (Object::Brush(x), Object::Brush(y)) => {
                    let Ok(Some(common)) = brush_intersect(&x.brush, &y.brush) else {
                        continue;
                    };
                    let volume = brush_geometry(&common).map_or(0.0, |g| g.volume());
                    if volume <= OVERLAP_EPS {
                        continue;
                    }
                    false
                }
                _ => true,
            };
            if pairs.len() == MAX_PAIRS {
                truncated = true;
                break 'outer;
            }
            pairs.push(json!({ "a": a.id(), "b": b.id(), "approx": approx }));
        }
    }
    let warnings = if truncated {
        vec![format!(
            "only the first {MAX_PAIRS} overlapping pairs are listed"
        )]
    } else {
        vec![]
    };
    Ok(Outcome {
        result: json!({ "pairs": pairs, "count": pairs.len() }),
        warnings,
    })
}
