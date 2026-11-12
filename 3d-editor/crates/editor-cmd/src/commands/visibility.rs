use std::collections::BTreeSet;

use editor_core::{Document, Flags, Object, ObjectId};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::results;
use super::util::non_empty;
use super::{CmdResult, NoArgs, Outcome};
use crate::ctx::Ctx;
use crate::error::CommandError;

fn flaggable(doc: &Document, id: ObjectId) -> Result<Object, CommandError> {
    match id {
        ObjectId::Camera(_) | ObjectId::Material(_) | ObjectId::ComponentDefs(_)
            if doc.contains(id) =>
        {
            Err(CommandError::wrong_kind(
                id,
                "brush, mesh, entity, group or layer",
            ))
        }
        _ => doc.get(id).ok_or_else(|| CommandError::not_found(id)),
    }
}

fn update_flags(
    ctx: &mut Ctx,
    ids: &[ObjectId],
    edit: impl Fn(Flags) -> Flags,
) -> CmdResult<results::Updated> {
    non_empty("ids", ids)?;
    let mut changed = Vec::new();
    for &id in ids {
        let object = flaggable(ctx.doc(), id)?;
        let old = object.flags().expect("flaggable objects have flags");
        let new = edit(old);
        if new != old && !changed.iter().any(|o: &Object| o.id() == id) {
            changed.push(object.with_flags(new));
        }
    }
    let updated = changed.len();
    for object in changed {
        ctx.insert(object);
    }
    Ok(Outcome::new(results::Updated { updated }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetHidden {
    pub ids: Vec<ObjectId>,
    pub hidden: bool,
}

pub fn set_hidden(ctx: &mut Ctx, args: SetHidden) -> CmdResult<results::Updated> {
    update_flags(ctx, &args.ids, |f| Flags {
        hidden: args.hidden,
        ..f
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetLocked {
    pub ids: Vec<ObjectId>,
    pub locked: bool,
}

pub fn set_locked(ctx: &mut Ctx, args: SetLocked) -> CmdResult<results::Updated> {
    update_flags(ctx, &args.ids, |f| Flags {
        locked: args.locked,
        ..f
    })
}

fn with_everything_below(doc: &Document, id: ObjectId, out: &mut BTreeSet<ObjectId>) {
    if out.insert(id)
        && matches!(
            id,
            ObjectId::Layer(_) | ObjectId::Group(_) | ObjectId::Entity(_)
        )
    {
        for child in doc.children(id) {
            with_everything_below(doc, child, out);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Isolate {
    pub ids: Vec<ObjectId>,
}

/// Hides everything except `ids`, what they contain and what contains them, at the highest level that leaves them shown.
pub fn isolate(ctx: &mut Ctx, args: Isolate) -> CmdResult<results::Updated> {
    non_empty("ids", &args.ids)?;
    let doc = ctx.doc();
    let mut keep = BTreeSet::new();
    for &id in &args.ids {
        flaggable(doc, id)?;
        with_everything_below(doc, id, &mut keep);
        keep.extend(doc.ancestors(id));
    }
    let mut hide = Vec::new();
    for id in doc.all_ids() {
        if keep.contains(&id)
            || matches!(
                id,
                ObjectId::Camera(_) | ObjectId::Material(_) | ObjectId::ComponentDefs(_)
            )
        {
            continue;
        }
        let Some(object) = doc.get(id) else { continue };
        let parents: Vec<ObjectId> = object
            .entity()
            .map(ObjectId::from)
            .into_iter()
            .chain(object.group().map(ObjectId::from))
            .chain(object.layer().map(ObjectId::from))
            .collect();
        let top = parents.iter().all(|p| keep.contains(p));
        let flags = object.flags().expect("not a camera");
        if top && !flags.hidden {
            hide.push(object.with_flags(Flags {
                hidden: true,
                ..flags
            }));
        }
    }
    let updated = hide.len();
    for object in hide {
        ctx.insert(object);
    }
    Ok(Outcome::new(results::Updated { updated }))
}

fn clear_all(ctx: &mut Ctx, edit: impl Fn(Flags) -> Flags) -> CmdResult<results::Updated> {
    let doc = ctx.doc();
    let changed: Vec<Object> = doc
        .all_ids()
        .into_iter()
        .filter_map(|id| doc.get(id))
        .filter_map(|o| {
            let old = o.flags()?;
            let new = edit(old);
            (new != old).then(|| o.with_flags(new))
        })
        .collect();
    let updated = changed.len();
    for object in changed {
        ctx.insert(object);
    }
    Ok(Outcome::new(results::Updated { updated }))
}

pub fn show_all(ctx: &mut Ctx, _: NoArgs) -> CmdResult<results::Updated> {
    clear_all(ctx, |f| Flags { hidden: false, ..f })
}

pub fn unlock_all(ctx: &mut Ctx, _: NoArgs) -> CmdResult<results::Updated> {
    clear_all(ctx, |f| Flags { locked: false, ..f })
}
