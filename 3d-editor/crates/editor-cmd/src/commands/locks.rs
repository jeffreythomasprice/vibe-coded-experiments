use std::collections::{BTreeMap, BTreeSet};

use editor_core::{Object, ObjectId};

use crate::ctx::Ctx;
use crate::error::CommandError;

type Lookup<'a> = dyn Fn(ObjectId) -> Option<Object> + 'a;

fn locked_in(get: &Lookup, id: ObjectId) -> bool {
    let mut pending = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(next) = pending.pop() {
        if !seen.insert(next) {
            continue;
        }
        let Some(object) = get(next) else { continue };
        if object.flags().is_some_and(|f| f.locked) {
            return true;
        }
        pending.extend(object.entity().map(ObjectId::from));
        pending.extend(object.group().map(ObjectId::from));
        pending.extend(object.layer().map(ObjectId::from));
    }
    false
}

fn only_flags_differ(before: &Object, after: &Object) -> bool {
    match after.flags() {
        Some(flags) => before.clone().with_flags(flags) == *after,
        None => false,
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
                if containers.into_iter().any(|c| locked_in(&post, c)) {
                    return Err(locked());
                }
            }
            (Some(_), None) => {
                if locked_in(&pre, id) {
                    return Err(locked());
                }
            }
            (Some(old), Some(new)) => {
                if only_flags_differ(old, new) {
                    continue;
                }
                if locked_in(&pre, id) || locked_in(&post, id) {
                    return Err(locked());
                }
            }
            (None, None) => {}
        }
    }
    Ok(())
}
