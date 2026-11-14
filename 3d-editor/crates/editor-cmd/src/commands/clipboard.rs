use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use editor_core::{Document, Layer, LayerId, MaterialId, Object, ObjectId, TextureSource};
use editor_geom::Aabb;
use glam::{DAffine3, DVec3};
use schemars::JsonSchema;
use serde::Deserialize;

use super::object::{Source, make_copy_from};
use super::results;
use super::util::{check_finite, descendants, non_empty, with_group, with_layer, world_object};
use super::{CmdResult, Outcome};
use crate::bounds::object_bounds;
use crate::ctx::Ctx;
use crate::error::CommandError;

const MAX_PASTE_OBJECTS: usize = 10_000;

pub struct Copied {
    pub text: String,
    pub count: usize,
}

fn without_entity(object: Object) -> Object {
    match object {
        Object::Brush(mut o) => {
            o.entity = None;
            Object::Brush(o)
        }
        Object::Mesh(mut o) => {
            o.entity = None;
            Object::Mesh(o)
        }
        other => other,
    }
}

fn face_materials(object: &Object) -> Vec<&str> {
    match object {
        Object::Brush(b) => b
            .brush
            .faces
            .iter()
            .map(|f| f.attrs.material.as_str())
            .collect(),
        Object::Mesh(m) => m
            .mesh
            .faces
            .iter()
            .map(|f| f.attrs.material.as_str())
            .collect(),
        _ => Vec::new(),
    }
}

pub fn copy_text(
    doc: &Document,
    scene_dir: Option<&Path>,
    ids: &[ObjectId],
) -> Result<Copied, CommandError> {
    non_empty("ids", ids)?;
    let mut all = BTreeSet::new();
    for &id in ids {
        world_object(doc, id)?;
        descendants(doc, id, &mut all);
    }
    let mut clip = Document::empty("clipboard", doc.next_id());
    let mut layers: BTreeSet<LayerId> = BTreeSet::new();
    let mut materials: BTreeSet<String> = BTreeSet::new();
    for &id in &all {
        let mut object = doc.get(id).expect("collected from the document");
        if object.group().is_some_and(|g| !all.contains(&g.into())) {
            object = with_group(object, None);
        }
        if let Object::Brush(b) = &object
            && b.entity.is_some_and(|e| !all.contains(&e.into()))
        {
            object = without_entity(object);
        } else if let Object::Mesh(m) = &object
            && m.entity.is_some_and(|e| !all.contains(&e.into()))
        {
            object = without_entity(object);
        }
        layers.extend(object.layer());
        materials.extend(face_materials(&object).into_iter().map(str::to_string));
        clip.insert(object);
    }
    for id in layers {
        if let Some(layer) = doc.layers.get(&id) {
            clip.layers.insert(
                id,
                Layer {
                    flags: Default::default(),
                    ..layer.clone()
                },
            );
        }
    }
    for name in materials {
        let Some(material) = doc.material_by_name(&name) else {
            continue;
        };
        let mut material = material.clone();
        if let (Some(TextureSource::Path { path }), Some(dir)) = (&mut material.texture, scene_dir)
            && Path::new(path.as_str()).is_relative()
        {
            *path = dir.join(path.as_str()).display().to_string();
        }
        clip.insert(Object::Material(material));
    }
    Ok(Copied {
        text: editor_fmt::write(&clip),
        count: all.len(),
    })
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Paste {
    /// Scene-format text, as clipboard.copy returns it or any scene file. Omit it to use the server's clipboard.
    text: Option<String>,
    /// Put the centre of the pasted objects' bounds here.
    #[schemars(with = "Option<[f64; 3]>")]
    at: Option<DVec3>,
    /// Shift the pasted objects by this much from where they were copied.
    #[schemars(with = "Option<[f64; 3]>")]
    offset: Option<DVec3>,
}

fn clip_world_ids(clip: &Document) -> Vec<ObjectId> {
    let mut ids: Vec<ObjectId> = Vec::new();
    ids.extend(clip.groups.keys().map(|&i| ObjectId::from(i)));
    ids.extend(clip.entities.keys().map(|&i| ObjectId::from(i)));
    ids.extend(clip.brushes.keys().map(|&i| ObjectId::from(i)));
    ids.extend(clip.meshes.keys().map(|&i| ObjectId::from(i)));
    ids
}

pub fn paste(ctx: &mut Ctx, args: Paste) -> CmdResult<results::Pasted> {
    let text = args.text.ok_or_else(|| {
        CommandError::invalid_arg("text", "the clipboard is empty")
            .with_hint("call clipboard.copy first, or pass scene-format text")
    })?;
    if args.at.is_some() && args.offset.is_some() {
        return Err(CommandError::invalid_arg(
            "at",
            "pass at or offset, not both",
        ));
    }
    if let Some(at) = args.at {
        check_finite("at", at)?;
    }
    if let Some(offset) = args.offset {
        check_finite("offset", offset)?;
    }
    let clip = editor_fmt::read(&text).map_err(|e| {
        CommandError::invalid_arg("text", e.to_string())
            .with_hint("text must be scene-format text, as clipboard.copy returns it")
    })?;
    let everything = clip_world_ids(&clip);
    if everything.is_empty() {
        return Err(CommandError::invalid_arg(
            "text",
            "the text holds no objects to paste",
        ));
    }
    if everything.len() > MAX_PASTE_OBJECTS {
        return Err(CommandError::invalid_arg(
            "text",
            format!(
                "this would create {} objects; the limit is {MAX_PASTE_OBJECTS}",
                everything.len()
            ),
        ));
    }
    let (roots, rest): (Vec<_>, Vec<_>) = everything.into_iter().partition(|&id| {
        let object = clip.get(id).expect("listed from the clip");
        object.group().is_none()
            && !matches!(&object, Object::Brush(b) if b.entity.is_some())
            && !matches!(&object, Object::Mesh(m) if m.entity.is_some())
    });
    let top_level = roots.len();
    let order: Vec<ObjectId> = roots.into_iter().chain(rest).collect();

    let offset = match args.at {
        Some(at) => {
            let bounds = order
                .iter()
                .filter_map(|&id| clip.get(id))
                .filter_map(|o| object_bounds(&o))
                .reduce(Aabb::merge);
            bounds.map_or(DVec3::ZERO, |b| at - (b.min + b.max) * 0.5)
        }
        None => args.offset.unwrap_or(DVec3::ZERO),
    };

    let mut warnings = Vec::new();
    let fallback = *ctx
        .doc()
        .layers
        .keys()
        .next()
        .expect("a document always has a layer");
    let layer_map: BTreeMap<LayerId, LayerId> = clip
        .layers
        .values()
        .map(|layer| {
            let target = ctx
                .doc()
                .layers
                .values()
                .find(|l| l.name == layer.name)
                .map_or(fallback, |l| l.id);
            (layer.id, target)
        })
        .collect();

    for material in clip.materials.values() {
        match ctx.doc().material_by_name(&material.name) {
            Some(existing) => {
                let mut same = existing.clone();
                same.id = material.id;
                if &same != material {
                    warnings.push(format!(
                        "material `{}` already exists and differs from the pasted one; kept the existing one",
                        material.name
                    ));
                }
            }
            None => {
                let mut created = material.clone();
                created.id = MaterialId(ctx.alloc_id());
                ctx.insert(Object::Material(created));
            }
        }
    }

    let shift = DAffine3::from_translation(offset);
    let (copies, ids) = make_copy_from(
        ctx,
        Source::Doc(&clip),
        &order,
        (offset != DVec3::ZERO).then_some(&shift),
    )?;
    for (copy, old) in copies.into_iter().zip(&order) {
        let layer = clip
            .get(*old)
            .and_then(|o| o.layer())
            .and_then(|l| layer_map.get(&l).copied())
            .unwrap_or(fallback);
        ctx.insert(with_layer(copy, layer));
    }
    let map = order.iter().copied().zip(ids.iter().copied()).collect();
    Ok(Outcome::with_warnings(
        results::Pasted {
            id: ids.first().copied(),
            ids,
            top_level,
            map,
        },
        warnings,
    ))
}
