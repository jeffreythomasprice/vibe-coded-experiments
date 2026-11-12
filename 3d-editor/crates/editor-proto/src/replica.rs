use std::collections::BTreeSet;

use editor_core::{Document, ElementRef, FaceId, Object, ObjectId, Part};

use crate::client::ServerEvent;
use crate::wire::{FileInfo, HistoryInfo};

#[derive(Debug, thiserror::Error)]
pub enum ReplicaError {
    #[error("the server sent an unreadable document: {0}")]
    Document(#[from] editor_fmt::LoadError),
}

/// A local copy of an attached session, kept current by the server's events.
#[derive(Debug, Clone)]
pub struct Replica {
    pub doc: Document,
    pub selection: BTreeSet<ObjectId>,
    pub face_selection: BTreeSet<(ObjectId, FaceId)>,
    pub revision: u64,
    pub file: FileInfo,
    pub history: HistoryInfo,
    pub session: String,
    pub closed: bool,
}

/// What an event changed, beyond the fields it overwrote.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Applied {
    /// The document was replaced or edited.
    pub document: bool,
    /// Component definitions, or the file they resolve against, changed.
    pub defs: bool,
}

impl Default for Replica {
    fn default() -> Self {
        Self {
            doc: Document::new("untitled"),
            selection: BTreeSet::new(),
            face_selection: BTreeSet::new(),
            revision: 0,
            file: FileInfo {
                path: None,
                name: String::new(),
                dirty: false,
            },
            history: HistoryInfo {
                can_undo: false,
                can_redo: false,
                transaction: None,
            },
            session: String::new(),
            closed: false,
        }
    }
}

impl Replica {
    pub fn apply(&mut self, event: ServerEvent) -> Result<Applied, ReplicaError> {
        let mut applied = Applied::default();
        match event {
            ServerEvent::Snapshot(s) => {
                self.doc = editor_fmt::read(&s.document_toml)?;
                self.selection = s.selection.into_iter().collect();
                self.face_selection = face_set(s.face_selection);
                self.revision = s.revision;
                self.file = s.file;
                self.history = s.history;
                self.session = s.session;
                applied.document = true;
                applied.defs = true;
            }
            ServerEvent::Changed(c) => {
                applied.document = !c.upsert.is_empty() || !c.deleted.is_empty();
                for object in c.upsert {
                    applied.defs |= matches!(object, Object::ComponentDefs(_));
                    self.doc.insert(object);
                }
                for id in c.deleted {
                    applied.defs |= matches!(id, ObjectId::ComponentDefs(_));
                    self.doc.remove(id);
                }
                if let Some(selection) = c.selection {
                    self.selection = selection.into_iter().collect();
                }
                if let Some(faces) = c.face_selection {
                    self.face_selection = face_set(faces);
                }
                if let Some(file) = c.file {
                    applied.defs |= file.path != self.file.path;
                    self.file = file;
                }
                if let Some(history) = c.history {
                    self.history = history;
                }
                self.revision = c.revision;
            }
            ServerEvent::Closed => self.closed = true,
        }
        Ok(applied)
    }
}

fn face_set(refs: Vec<ElementRef>) -> BTreeSet<(ObjectId, FaceId)> {
    refs.into_iter()
        .filter_map(|r| match r.part {
            Part::Face(f) => Some((r.object, f)),
            _ => None,
        })
        .collect()
}
