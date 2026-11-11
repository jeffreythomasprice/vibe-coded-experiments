use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use editor_core::{Document, Object, ObjectId};

pub type Journal = Vec<(ObjectId, Option<Object>)>;

#[derive(Debug, Clone, Copy)]
pub struct Savepoint {
    pub journal_len: usize,
    pub next_id: u64,
}

pub struct Ctx<'a> {
    doc: &'a mut Document,
    journal: &'a mut Journal,
    selection: &'a BTreeSet<ObjectId>,
    scene_dir: Option<PathBuf>,
}

impl<'a> Ctx<'a> {
    pub fn new(
        doc: &'a mut Document,
        journal: &'a mut Journal,
        selection: &'a BTreeSet<ObjectId>,
    ) -> Self {
        Self {
            doc,
            journal,
            selection,
            scene_dir: None,
        }
    }

    pub fn with_scene_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.scene_dir = dir;
        self
    }

    /// The directory of the scene file; `None` until the scene is saved.
    pub fn scene_dir(&self) -> Option<&Path> {
        self.scene_dir.as_deref()
    }

    pub fn selection(&self) -> &BTreeSet<ObjectId> {
        self.selection
    }

    pub fn doc(&self) -> &Document {
        self.doc
    }

    pub fn journal_len(&self) -> usize {
        self.journal.len()
    }

    pub fn journal_since(&self, start: usize) -> &[(ObjectId, Option<Object>)] {
        &self.journal[start..]
    }

    pub fn alloc_id(&mut self) -> u64 {
        self.doc.alloc_id()
    }

    pub fn insert(&mut self, object: Object) {
        let id = object.id();
        let before = self.doc.insert(object);
        self.journal.push((id, before));
    }

    pub fn remove(&mut self, id: ObjectId) -> Option<Object> {
        let before = self.doc.remove(id);
        if before.is_some() {
            self.journal.push((id, before.clone()));
        }
        before
    }
}

pub fn savepoint(doc: &Document, journal: &Journal) -> Savepoint {
    Savepoint {
        journal_len: journal.len(),
        next_id: doc.next_id(),
    }
}

pub fn rollback(doc: &mut Document, journal: &mut Journal, sp: Savepoint) {
    while journal.len() > sp.journal_len {
        let (id, before) = journal.pop().expect("length checked");
        match before {
            Some(object) => {
                doc.insert(object);
            }
            None => {
                doc.remove(id);
            }
        }
    }
    doc.set_next_id(sp.next_id);
}
