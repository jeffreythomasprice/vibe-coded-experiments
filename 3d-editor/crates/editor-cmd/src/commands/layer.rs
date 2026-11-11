use editor_core::{Extra, Layer, LayerId, Object, ObjectId};
use schemars::JsonSchema;
use serde::Deserialize;

use super::results;
use super::util::with_layer;
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

pub fn check_layer_name(ctx: &Ctx, name: &str, own: Option<LayerId>) -> Result<(), CommandError> {
    if name.trim().is_empty() {
        return Err(CommandError::invalid_arg("name", "name must not be empty"));
    }
    if ctx
        .doc()
        .layers
        .values()
        .any(|l| l.name == name && Some(l.id) != own)
    {
        return Err(CommandError::invalid_arg(
            "name",
            "a layer with this name already exists",
        ));
    }
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Create {
    name: String,
}

pub fn create(ctx: &mut Ctx, args: Create) -> CmdResult<results::Created> {
    check_layer_name(ctx, &args.name, None)?;
    let id = LayerId(ctx.alloc_id());
    ctx.insert(Object::Layer(Layer {
        id,
        name: args.name,
        extra: Extra::new(),
    }));
    Ok(Outcome::created(vec![id.into()], vec![]))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Delete {
    id: LayerId,
    move_to: Option<LayerId>,
}

pub fn delete(ctx: &mut Ctx, args: Delete) -> CmdResult<results::LayerDeleted> {
    let doc = ctx.doc();
    if !doc.layers.contains_key(&args.id) {
        return Err(CommandError::not_found(args.id));
    }
    if doc.layers.len() == 1 {
        return Err(
            CommandError::invalid_arg("id", "a document needs at least one layer")
                .with_hint("create another layer first"),
        );
    }
    let members = doc.children(args.id.into());
    let mut moved = 0;
    if !members.is_empty() {
        let Some(target) = args.move_to else {
            return Err(CommandError::invalid_arg(
                "move_to",
                format!("{} still contains {} objects", args.id, members.len()),
            )
            .with_hint(
                "pass move_to with the layer that should receive them, or delete them first",
            ));
        };
        if target == args.id || !doc.layers.contains_key(&target) {
            return Err(if target == args.id {
                CommandError::invalid_arg("move_to", "move_to must be a different layer")
            } else {
                CommandError::not_found(target)
            });
        }
        // Group members and brush entity members share their parent's layer, so move every object on the layer.
        let on_layer: Vec<ObjectId> = doc
            .all_ids()
            .into_iter()
            .filter(|&id| doc.get(id).and_then(|o| o.layer()) == Some(args.id))
            .collect();
        for id in on_layer {
            if let Some(object) = ctx.doc().get(id) {
                ctx.insert(with_layer(object, target));
                moved += 1;
            }
        }
    }
    ctx.remove(args.id.into());
    Ok(Outcome::new(results::LayerDeleted { deleted: 1, moved }))
}
