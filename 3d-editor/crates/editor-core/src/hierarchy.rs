use std::collections::HashMap;

use crate::{Document, GroupId, ObjectId};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HierarchyError {
    #[error("{id} reuses the number of {other}")]
    DuplicateNumber { id: ObjectId, other: ObjectId },
    #[error("{id}.{field} refers to missing {target}")]
    DanglingReference {
        id: ObjectId,
        field: &'static str,
        target: ObjectId,
    },
    #[error("group {0} is its own ancestor")]
    GroupCycle(GroupId),
}

pub fn check(doc: &Document) -> Result<(), HierarchyError> {
    let mut seen: HashMap<u64, ObjectId> = HashMap::new();
    for id in doc.all_ids() {
        if let Some(&other) = seen.get(&id.number()) {
            return Err(HierarchyError::DuplicateNumber { id, other });
        }
        seen.insert(id.number(), id);
    }

    for id in doc.all_ids() {
        let Some(obj) = doc.get(id) else { continue };
        if let Some(layer) = obj.layer()
            && !doc.layers.contains_key(&layer)
        {
            return Err(dangling(id, "layer", layer.into()));
        }
        if let Some(group) = obj.group()
            && !doc.groups.contains_key(&group)
        {
            return Err(dangling(id, "group", group.into()));
        }
        if let Some(entity) = obj.entity()
            && !doc.entities.contains_key(&entity)
        {
            return Err(dangling(id, "entity", entity.into()));
        }
    }

    for &start in doc.groups.keys() {
        let mut cur = doc.groups[&start].parent;
        let mut steps = 0;
        while let Some(g) = cur {
            if g == start || steps > doc.groups.len() {
                return Err(HierarchyError::GroupCycle(start));
            }
            cur = doc.groups[&g].parent;
            steps += 1;
        }
    }
    Ok(())
}

fn dangling(id: ObjectId, field: &'static str, target: ObjectId) -> HierarchyError {
    HierarchyError::DanglingReference { id, field, target }
}
