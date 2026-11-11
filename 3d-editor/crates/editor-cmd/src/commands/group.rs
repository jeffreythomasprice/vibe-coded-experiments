use std::collections::BTreeSet;

use editor_core::{Extra, Group, GroupId, Object, ObjectId};
use schemars::JsonSchema;
use serde::Deserialize;

use super::results;
use super::util::{non_empty, set_group, world_object};
use super::{CmdResult, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

fn reject_entity_members(object: &Object) -> Result<(), CommandError> {
    if let Some(entity) = object.entity() {
        return Err(CommandError::invalid_arg(
            "objects",
            format!("{} belongs to {entity}", object.id()),
        )
        .with_hint("group the entity instead, or call entity.release_objects first"));
    }
    Ok(())
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Create {
    objects: Vec<ObjectId>,
    name: Option<String>,
}

pub fn create(ctx: &mut Ctx, args: Create) -> CmdResult<results::Created> {
    non_empty("objects", &args.objects)?;
    let name = args.name.unwrap_or_else(|| "group".into());
    if name.trim().is_empty() {
        return Err(CommandError::invalid_arg("name", "name must not be empty"));
    }
    let mut members = Vec::new();
    for &id in &args.objects {
        let object = world_object(ctx.doc(), id)?;
        reject_entity_members(&object)?;
        members.push(object);
    }
    let first = &members[0];
    let (layer, parent) = (first.layer().expect("has a layer"), first.group());
    if let Some(odd) = members
        .iter()
        .find(|m| m.layer() != Some(layer) || m.group() != parent)
    {
        return Err(CommandError::invalid_arg(
            "objects",
            format!(
                "{} is not on the same layer and group as {}",
                odd.id(),
                first.id()
            ),
        )
        .with_hint("use object.set_layer or group.move to line them up"));
    }

    let id = GroupId(ctx.alloc_id());
    ctx.insert(Object::Group(Group {
        flags: Default::default(),
        id,
        name,
        layer,
        parent,
        extra: Extra::new(),
    }));
    for &member in &args.objects {
        set_group(ctx, member, Some(id));
    }
    Ok(Outcome::created(vec![id.into()], vec![]))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Ungroup {
    ids: Vec<GroupId>,
}

pub fn ungroup(ctx: &mut Ctx, args: Ungroup) -> CmdResult<results::Ungrouped> {
    non_empty("ids", &args.ids)?;
    for g in &args.ids {
        if !ctx.doc().groups.contains_key(g) {
            return Err(CommandError::not_found(g));
        }
    }
    let mut released = 0;
    for g in &args.ids {
        let Some(group) = ctx.doc().groups.get(g).cloned() else {
            continue;
        };
        let children = ctx.doc().children((*g).into());
        for child in children {
            set_group(ctx, child, group.parent);
            released += 1;
        }
        ctx.remove((*g).into());
    }
    Ok(Outcome::new(results::Ungrouped {
        ungrouped: args.ids.len(),
        released,
    }))
}

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Move {
    objects: Vec<ObjectId>,
    /// The new parent group; null or absent removes the objects from their group.
    #[serde(default)]
    group: Option<GroupId>,
}

pub fn move_objects(ctx: &mut Ctx, args: Move) -> CmdResult<results::Moved> {
    non_empty("objects", &args.objects)?;
    let target = match args.group {
        Some(g) => Some(
            ctx.doc()
                .groups
                .get(&g)
                .cloned()
                .ok_or_else(|| CommandError::not_found(g))?,
        ),
        None => None,
    };

    let mut moving = BTreeSet::new();
    for &id in &args.objects {
        let object = world_object(ctx.doc(), id)?;
        reject_entity_members(&object)?;
        if let Some(t) = &target {
            if object.layer() != Some(t.layer) {
                return Err(CommandError::invalid_arg(
                    "group",
                    format!("{id} is on a different layer than {}", t.id),
                )
                .with_hint("call object.set_layer first"));
            }
            if let ObjectId::Group(moving_group) = id {
                let mut cur = Some(t.id);
                while let Some(g) = cur {
                    if g == moving_group {
                        return Err(CommandError::invalid_arg(
                            "group",
                            format!("{id} cannot be moved into itself or one of its own subgroups"),
                        ));
                    }
                    cur = ctx.doc().groups.get(&g).and_then(|x| x.parent);
                }
            }
        }
        moving.insert(id);
    }
    for id in moving {
        set_group(ctx, id, target.as_ref().map(|t| t.id));
    }
    Ok(Outcome::new(results::Moved {
        moved: args.objects.len(),
    }))
}
