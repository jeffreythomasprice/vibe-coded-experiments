use std::collections::HashMap;

use editor_core::{Document, Object, ObjectId};

use crate::envelope::Changes;

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub id: ObjectId,
    pub before: Option<Object>,
    pub after: Option<Object>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HistoryEntry {
    pub label: String,
    pub client: String,
    pub conn: u64,
    pub merge_key: Option<String>,
    pub revision: u64,
    pub changes: Vec<Change>,
}

pub fn collapse(doc: &Document, journal: &[(ObjectId, Option<Object>)]) -> Vec<Change> {
    let mut order: Vec<ObjectId> = Vec::new();
    let mut first: HashMap<ObjectId, &Option<Object>> = HashMap::new();
    for (id, before) in journal {
        if !first.contains_key(id) {
            first.insert(*id, before);
            order.push(*id);
        }
    }
    order
        .into_iter()
        .filter_map(|id| {
            let before = first[&id].clone();
            let after = doc.get(id);
            (before != after).then_some(Change { id, before, after })
        })
        .collect()
}

pub fn changes_of(changes: &[Change], reverse: bool) -> Changes {
    let mut out = Changes::default();
    for c in changes {
        let (before, after) = if reverse {
            (&c.after, &c.before)
        } else {
            (&c.before, &c.after)
        };
        match (before, after) {
            (None, Some(_)) => out.created.push(c.id),
            (Some(_), None) => out.deleted.push(c.id),
            (Some(_), Some(_)) => out.modified.push(c.id),
            (None, None) => {}
        }
    }
    out
}

pub fn apply(doc: &mut Document, changes: &[Change], reverse: bool) {
    for c in changes {
        let target = if reverse { &c.before } else { &c.after };
        match target {
            Some(object) => {
                doc.insert(object.clone());
            }
            None => {
                doc.remove(c.id);
            }
        }
    }
}

/// Folds `new` into `into`, keeping each object's earliest `before` and latest `after`.
pub fn merge(into: &mut Vec<Change>, new: Vec<Change>) {
    for c in new {
        match into.iter_mut().find(|e| e.id == c.id) {
            Some(existing) => existing.after = c.after,
            None => into.push(c),
        }
    }
    into.retain(|c| c.before != c.after);
}
