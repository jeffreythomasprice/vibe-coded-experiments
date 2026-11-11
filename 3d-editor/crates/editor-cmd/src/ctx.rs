use std::collections::BTreeSet;

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
        }
    }

    pub fn selection(&self) -> &BTreeSet<ObjectId> {
        self.selection
    }

    pub fn doc(&self) -> &Document {
        self.doc
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
