use std::collections::{BTreeMap, BTreeSet};

use editor_core::{GroupId, Object, ObjectId};

use crate::ctx::Ctx;
use crate::error::CommandError;

type Lookup<'a> = dyn Fn(ObjectId) -> Option<Object> + 'a;

fn containers(get: &Lookup, id: ObjectId) -> BTreeSet<ObjectId> {
    let mut pending = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(next) = pending.pop() {
        if !seen.insert(next) {
            continue;
        }
        let Some(object) = get(next) else { continue };
        pending.extend(object.entity().map(ObjectId::from));
        pending.extend(object.group().map(ObjectId::from));
        pending.extend(object.layer().map(ObjectId::from));
    }
    seen
}

fn locked_in(get: &Lookup, open: Option<GroupId>, id: ObjectId) -> bool {
    let chain = containers(get, id);
    if chain
        .iter()
        .any(|&c| get(c).and_then(|o| o.flags()).is_some_and(|f| f.locked))
    {
        return true;
    }
    let Some(open) = open.map(ObjectId::from) else {
        return false;
    };
    matches!(
        id,
        ObjectId::Brush(_) | ObjectId::Mesh(_) | ObjectId::Entity(_) | ObjectId::Group(_)
    ) && !chain.contains(&open)
        && !containers(get, open).contains(&id)
}

fn open_group(get: &Lookup, groups: &BTreeSet<GroupId>) -> Option<GroupId> {
    let open: Vec<_> = groups
        .iter()
        .filter_map(|&g| match get(g.into()) {
            Some(Object::Group(group)) if group.open => Some(group),
            _ => None,
        })
        .collect();
    let parents: BTreeSet<GroupId> = open.iter().filter_map(|g| g.parent).collect();
    open.iter().find(|g| !parents.contains(&g.id)).map(|g| g.id)
}

fn only_flags_differ(before: &Object, after: &Object) -> bool {
    match (before, after) {
        (Object::Group(b), Object::Group(a)) => {
            let mut same = b.clone();
            same.flags = a.flags;
            same.open = a.open;
            same == *a
        }
        _ => match after.flags() {
            Some(flags) => before.clone().with_flags(flags) == *after,
            None => false,
        },
    }
}

/// Puts objects the op created at the top level into the open group, so creating while a group is open works.
pub fn adopt(ctx: &mut Ctx, start: usize) {
    let Some(open) = ctx.doc().open_group() else {
        return;
    };
    let Some(layer) = ctx.doc().groups.get(&open).map(|g| g.layer) else {
        return;
    };
    let created: Vec<ObjectId> = ctx
        .journal_since(start)
        .iter()
        .filter(|(_, old)| old.is_none())
        .map(|(id, _)| *id)
        .collect();
    let mut adopted = BTreeSet::new();
    for id in &created {
        let Some(object) = ctx.doc().get(*id) else {
            continue;
        };
        let replacement = match object {
            Object::Brush(mut o) if o.entity.is_none() && o.group.is_none() => {
                o.group = Some(open);
                o.layer = layer;
                Object::Brush(o)
            }
            Object::Mesh(mut o) if o.entity.is_none() && o.group.is_none() => {
                o.group = Some(open);
                o.layer = layer;
                Object::Mesh(o)
            }
            Object::Entity(mut o) if o.group.is_none() => {
                o.group = Some(open);
                o.layer = layer;
                Object::Entity(o)
            }
            Object::Group(mut o) if o.parent.is_none() && o.id != open => {
                o.parent = Some(open);
                o.layer = layer;
                Object::Group(o)
            }
            _ => continue,
        };
        adopted.insert(*id);
        ctx.insert(replacement);
    }
    for id in &created {
        let Some(object) = ctx.doc().get(*id) else {
            continue;
        };
        let owner = object.entity().map(ObjectId::from);
        if owner.is_some_and(|e| adopted.contains(&e)) {
            match object {
                Object::Brush(mut o) => {
                    o.layer = layer;
                    ctx.insert(Object::Brush(o));
                }
                Object::Mesh(mut o) => {
                    o.layer = layer;
                    ctx.insert(Object::Mesh(o));
                }
                _ => {}
            }
        }
    }
}

/// Fails with `Locked` when the journal since `start` edits or deletes a locked object, or puts one into a locked container.
pub fn enforce(ctx: &Ctx, start: usize) -> Result<(), CommandError> {
    let mut before: BTreeMap<ObjectId, Option<Object>> = BTreeMap::new();
    for (id, object) in ctx.journal_since(start) {
        before.entry(*id).or_insert_with(|| object.clone());
    }
    let doc = ctx.doc();
    let pre = |id: ObjectId| match before.get(&id) {
        Some(entry) => entry.clone(),
        None => doc.get(id),
    };
    let post = |id: ObjectId| doc.get(id);
    let groups: BTreeSet<GroupId> = doc
        .groups
        .keys()
        .copied()
        .chain(before.keys().filter_map(|id| match id {
            ObjectId::Group(g) => Some(*g),
            _ => None,
        }))
        .collect();
    let (open_pre, open_post) = (open_group(&pre, &groups), open_group(&post, &groups));

    for (&id, old) in &before {
        let new = doc.get(id);
        let locked = || CommandError::Locked {
            target: id.to_string(),
        };
        match (old, &new) {
            (None, Some(created)) => {
                let containers = created
                    .entity()
                    .map(ObjectId::from)
                    .into_iter()
                    .chain(created.group().map(ObjectId::from))
                    .chain(created.layer().map(ObjectId::from));
                if containers
                    .into_iter()
                    .any(|c| locked_in(&post, open_post, c))
                {
                    return Err(locked());
                }
            }
            (Some(_), None) => {
                if locked_in(&pre, open_pre, id) {
                    return Err(locked());
                }
            }
            (Some(old), Some(new)) => {
                if only_flags_differ(old, new) {
                    continue;
                }
                if locked_in(&pre, open_pre, id) || locked_in(&post, open_post, id) {
                    return Err(locked());
                }
            }
            (None, None) => {}
        }
    }
    Ok(())
}
