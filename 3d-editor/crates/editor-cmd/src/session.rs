use std::collections::BTreeSet;
use std::path::PathBuf;

use editor_core::{Document, ElementRef, FaceId, Object, ObjectId, Part};
use serde_json::Value;

use crate::commands::session_args::{
    Begin, List, New, Open, Save, SelectFaces, SelectIds, SelectKind,
};
use crate::commands::{self, Exec, NoArgs, OpInfo, parse_args, results};
use crate::ctx::{Ctx, Journal, Savepoint, rollback, savepoint};
use crate::envelope::{Changes, Request, Response, Success};
use crate::error::CommandError;
use crate::history::{HistoryEntry, apply, changes_of, collapse, merge};

/// Who is issuing a request: `conn` owns transactions and merge chains, `name` is the attribution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub conn: u64,
    pub name: String,
}

impl Actor {
    pub fn new(conn: u64, name: impl Into<String>) -> Self {
        Self {
            conn,
            name: name.into(),
        }
    }

    pub fn local(name: &str) -> Self {
        Self::new(0, name)
    }
}

struct OpenTxn {
    label: String,
    actor: Actor,
    start: Savepoint,
    start_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileState {
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxnInfo {
    pub holder: String,
    pub conn: u64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
    pub transaction: Option<TxnInfo>,
}

/// What changed since the last `take_feed`: objects to re-read from the document, and flags for session state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Feed {
    pub ids: BTreeSet<ObjectId>,
    pub replaced: bool,
    pub selection: bool,
    pub face_selection: bool,
    pub file: bool,
    pub history: bool,
}

impl Feed {
    pub fn is_empty(&self) -> bool {
        *self == Feed::default()
    }
}

pub struct Session {
    doc: Document,
    revision: u64,
    journal: Journal,
    open: Option<OpenTxn>,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    selection: BTreeSet<ObjectId>,
    face_selection: BTreeSet<(ObjectId, FaceId)>,
    path: Option<PathBuf>,
    saved_revision: u64,
    feed: Feed,
}

fn selectable(id: ObjectId) -> bool {
    matches!(
        id,
        ObjectId::Brush(_) | ObjectId::Mesh(_) | ObjectId::Entity(_) | ObjectId::Group(_)
    )
}

fn create_parent_dirs(path: &std::path::Path) -> Result<(), CommandError> {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => {
            std::fs::create_dir_all(dir).map_err(|e| CommandError::Io {
                message: format!("{}: {e}", dir.display()),
            })
        }
        _ => Ok(()),
    }
}

fn stem_name(path: &std::path::Path) -> Option<String> {
    let file = path.file_name()?.to_string_lossy();
    Some(file.trim_end_matches(".scene.toml").to_string())
}

fn file_error(e: editor_fmt::FileError) -> CommandError {
    match e {
        editor_fmt::FileError::Io { .. } => CommandError::Io {
            message: e.to_string(),
        },
        editor_fmt::FileError::Load { .. } => CommandError::Format {
            message: e.to_string(),
        },
    }
}

impl Session {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            revision: 0,
            journal: Journal::new(),
            open: None,
            undo: Vec::new(),
            redo: Vec::new(),
            selection: BTreeSet::new(),
            face_selection: BTreeSet::new(),
            path: None,
            saved_revision: 0,
            feed: Feed::default(),
        }
    }

    pub fn with_path(doc: Document, path: PathBuf) -> Self {
        let mut s = Self::new(doc);
        s.path = Some(path);
        s
    }

    pub fn path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }

    pub fn selection(&self) -> &BTreeSet<ObjectId> {
        &self.selection
    }

    pub fn face_selection(&self) -> &BTreeSet<(ObjectId, FaceId)> {
        &self.face_selection
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    /// The directory relative texture paths resolve against.
    pub fn scene_dir(&self) -> Option<PathBuf> {
        self.path.as_deref().and_then(Self::scene_dir_of)
    }

    pub fn has_open_transaction(&self) -> bool {
        self.open.is_some()
    }

    /// Replaces the document with recovered work: it keeps the session's file and stays unsaved.
    pub fn recover(&mut self, doc: Document, discard: bool) -> Result<(), CommandError> {
        self.guard_replace(discard)?;
        let path = self.path.clone();
        self.fresh_document(doc, path);
        self.saved_revision = self.revision - 1;
        self.feed.file = true;
        self.feed.history = true;
        self.feed.selection = true;
        self.feed.face_selection = true;
        Ok(())
    }

    pub fn document(&self) -> &Document {
        &self.doc
    }

    pub fn into_document(self) -> Document {
        self.doc
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn file_state(&self) -> FileState {
        FileState {
            path: self.path.as_ref().map(|p| p.display().to_string()),
            name: self.doc.name.clone(),
            dirty: self.is_dirty(),
        }
    }

    pub fn history_state(&self) -> HistoryState {
        HistoryState {
            can_undo: !self.undo.is_empty(),
            can_redo: !self.redo.is_empty(),
            transaction: self.open.as_ref().map(|t| TxnInfo {
                holder: t.actor.name.clone(),
                conn: t.actor.conn,
                label: t.label.clone(),
            }),
        }
    }

    pub fn take_feed(&mut self) -> Feed {
        std::mem::take(&mut self.feed)
    }

    pub fn execute_value(&mut self, request: Value, client: &str) -> Response {
        match Request::from_value(request) {
            Ok(r) => self.execute(r, client),
            Err(e) => Response::error(&e),
        }
    }

    pub fn execute(&mut self, request: Request, client: &str) -> Response {
        self.execute_as(request, &Actor::local(client))
    }

    pub fn execute_as(&mut self, request: Request, actor: &Actor) -> Response {
        let selection = self.selection.clone();
        let face_selection = self.face_selection.clone();
        let file = self.file_state();
        let history = self.history_state();
        let result = self.run(request, actor);
        self.feed.selection |= self.selection != selection;
        self.feed.face_selection |= self.face_selection != face_selection;
        self.feed.file |= self.file_state() != file;
        self.feed.history |= self.history_state() != history;
        match result {
            Ok(success) => Response::Success(success),
            Err(e) => Response::error(&e),
        }
    }

    /// Rolls back the transaction `conn` has open, if any. Returns whether there was one.
    pub fn abandon(&mut self, conn: u64) -> bool {
        if self.open.as_ref().is_none_or(|t| t.actor.conn != conn) {
            return false;
        }
        let history = self.history_state();
        let file = self.file_state();
        let _ = self.rollback_open();
        self.prune_selection();
        self.feed.history |= self.history_state() != history;
        self.feed.file |= self.file_state() != file;
        true
    }

    fn rollback_open(&mut self) -> Result<Changes, CommandError> {
        let txn = self.open.take().ok_or(CommandError::NoTransaction)?;
        let entries = collapse(&self.doc, &self.journal);
        let changes = changes_of(&entries, true);
        self.feed.ids.extend(entries.iter().map(|c| c.id));
        rollback(&mut self.doc, &mut self.journal, txn.start);
        if self.revision != txn.start_revision {
            let was_clean = self.saved_revision == txn.start_revision;
            self.revision += 1;
            if was_clean {
                self.saved_revision = self.revision;
            }
        }
        Ok(changes)
    }

    fn check_not_busy(&self, op: &str, actor: &Actor) -> Result<(), CommandError> {
        let Some(txn) = &self.open else {
            return Ok(());
        };
        if txn.actor.conn == actor.conn {
            return Ok(());
        }
        let Some(info) = OpInfo::find(op) else {
            return Ok(());
        };
        let blocked = match info.exec {
            Exec::Document => info.safety != crate::Safety::Read,
            Exec::Session => {
                (op.starts_with("history.") && op != "history.list")
                    || (op.starts_with("file.") && op != "file.info")
            }
            Exec::Client | Exec::Server => false,
        };
        if blocked {
            return Err(CommandError::SessionBusy {
                holder: txn.actor.name.clone(),
                label: txn.label.clone(),
            });
        }
        Ok(())
    }

    fn success<R: serde::Serialize>(
        &self,
        result: R,
        changes: Changes,
        warnings: Vec<String>,
    ) -> Success {
        Success {
            ok: true,
            revision: self.revision,
            result: serde_json::to_value(result).expect("results serialize"),
            changes,
            warnings,
        }
    }

    fn run(&mut self, mut request: Request, actor: &Actor) -> Result<Success, CommandError> {
        if let Some(info) = OpInfo::find(&request.op) {
            request.op = info.name.to_string();
        }
        self.check_not_busy(&request.op, actor)?;
        if let Some(expected) = request.expect_revision
            && expected != self.revision
        {
            return Err(CommandError::RevisionMismatch {
                expected,
                actual: self.revision,
            });
        }

        if OpInfo::find(&request.op).is_some_and(|o| o.exec == Exec::Session) {
            if request.dry_run {
                return Err(CommandError::invalid_arg(
                    "dry_run",
                    "session operations cannot be dry-run",
                ));
            }
            let result = self.session_op(&request.op, request.args, actor);
            self.prune_selection();
            return result;
        }

        if OpInfo::find(&request.op).is_some_and(|o| o.exec == Exec::Server) {
            return Err(commands::unavailable(&request.op));
        }

        let sp = savepoint(&self.doc, &self.journal);
        let outcome = {
            let scene_dir = self
                .path
                .as_deref()
                .and_then(|p| p.parent())
                .map(PathBuf::from);
            let mut ctx = Ctx::new(&mut self.doc, &mut self.journal, &self.selection)
                .with_scene_dir(scene_dir);
            commands::dispatch(&mut ctx, &request.op, request.args)
        };
        let outcome = match outcome {
            Ok(o) => o,
            Err(e) => {
                rollback(&mut self.doc, &mut self.journal, sp);
                return Err(e);
            }
        };

        let entries = collapse(&self.doc, &self.journal[sp.journal_len..]);
        let changes = changes_of(&entries, false);

        if request.dry_run {
            rollback(&mut self.doc, &mut self.journal, sp);
            return Ok(self.success(outcome.result, changes, outcome.warnings));
        }

        self.feed.ids.extend(entries.iter().map(|c| c.id));
        if self.open.is_some() {
            if !entries.is_empty() {
                self.revision += 1;
            }
        } else {
            self.journal.clear();
            if !entries.is_empty() {
                self.revision += 1;
                self.redo.clear();
                self.record(request.op, request.merge_key, actor, entries);
            }
        }
        self.prune_selection();
        Ok(self.success(outcome.result, changes, outcome.warnings))
    }

    fn record(
        &mut self,
        label: String,
        merge_key: Option<String>,
        actor: &Actor,
        entries: Vec<crate::history::Change>,
    ) {
        let revision = self.revision;
        if let Some(key) = &merge_key
            && let Some(top) = self.undo.last_mut()
            && top.conn == actor.conn
            && top.merge_key.as_ref() == Some(key)
        {
            merge(&mut top.changes, entries);
            top.revision = revision;
            if top.changes.is_empty() {
                self.undo.pop();
            }
            return;
        }
        self.undo.push(HistoryEntry {
            label,
            client: actor.name.clone(),
            conn: actor.conn,
            merge_key,
            revision,
            changes: entries,
        });
    }

    fn scene_dir_of(path: &std::path::Path) -> Option<PathBuf> {
        let dir = path.parent()?;
        Some(std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf()))
    }

    /// Saving into another directory keeps relative texture paths pointing at the same files.
    fn rebase_textures(&mut self, target: &std::path::Path) -> Option<(PathBuf, PathBuf)> {
        let old_dir = Self::scene_dir_of(self.path.as_deref()?)?;
        let new_dir = Self::scene_dir_of(target)?;
        if old_dir == new_dir {
            return None;
        }
        self.rebase_textures_between(&old_dir, &new_dir);
        Some((old_dir, new_dir))
    }

    fn rebase_textures_between(&mut self, old_dir: &std::path::Path, new_dir: &std::path::Path) {
        let rebase = |object: &mut Object| {
            if let Object::Material(m) = object
                && let Some(editor_core::TextureSource::Path { path }) = &mut m.texture
            {
                *path = crate::texture::rebase(path, old_dir, new_dir);
            }
        };
        let ids: Vec<ObjectId> = self.doc.materials.keys().map(|&m| m.into()).collect();
        for id in ids {
            if let Some(mut object) = self.doc.get(id) {
                let before = object.clone();
                rebase(&mut object);
                if object != before {
                    self.doc.insert(object);
                    self.feed.ids.insert(id);
                }
            }
        }
        for entry in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            for change in &mut entry.changes {
                change.before.iter_mut().for_each(rebase);
                change.after.iter_mut().for_each(rebase);
            }
        }
    }

    fn prune_selection(&mut self) {
        let doc = &self.doc;
        self.selection
            .retain(|id| doc.contains(*id) && !doc.effective_hidden(*id));
        self.face_selection.retain(|(id, face)| {
            let exists = match id {
                ObjectId::Brush(b) => doc
                    .brushes
                    .get(b)
                    .is_some_and(|o| o.brush.faces.iter().any(|f| f.id == *face)),
                ObjectId::Mesh(m) => doc
                    .meshes
                    .get(m)
                    .is_some_and(|o| o.mesh.faces.iter().any(|f| f.id == *face)),
                _ => false,
            };
            exists && !doc.effective_hidden(*id)
        });
    }

    fn face_selection_result(&self) -> results::FaceSelection {
        let faces: Vec<ElementRef> = self
            .face_selection
            .iter()
            .map(|&(object, face)| ElementRef {
                object,
                part: Part::Face(face),
            })
            .collect();
        results::FaceSelection {
            count: faces.len(),
            faces,
        }
    }

    fn expand_faces(
        &self,
        refs: &[ElementRef],
        adding: bool,
    ) -> Result<BTreeSet<(ObjectId, FaceId)>, CommandError> {
        let mut out = BTreeSet::new();
        for r in refs {
            let all: Vec<FaceId> = match r.object {
                ObjectId::Brush(b) => self
                    .doc
                    .brushes
                    .get(&b)
                    .map(|o| o.brush.faces.iter().map(|f| f.id).collect()),
                ObjectId::Mesh(m) => self
                    .doc
                    .meshes
                    .get(&m)
                    .map(|o| o.mesh.faces.iter().map(|f| f.id).collect()),
                _ if self.doc.contains(r.object) => {
                    return Err(CommandError::wrong_kind(r.object, "brush or mesh"));
                }
                _ => None,
            }
            .ok_or_else(|| CommandError::not_found(r.object))?;
            if adding && self.doc.effective_hidden(r.object) {
                return Err(
                    CommandError::invalid_arg("faces", format!("{} is hidden", r.object))
                        .with_hint("show it first with object.set_hidden or object.show_all"),
                );
            }
            match r.part {
                Part::All => out.extend(all.into_iter().map(|f| (r.object, f))),
                Part::Face(f) if all.contains(&f) => {
                    out.insert((r.object, f));
                }
                Part::Face(_) => return Err(CommandError::not_found(r)),
                Part::Vertex(_) => {
                    return Err(CommandError::invalid_arg(
                        "faces",
                        format!("{r} is a vertex; faces look like b12/f3"),
                    ));
                }
            }
        }
        Ok(out)
    }

    fn selection_result(&self) -> results::IdCount {
        let mut ids: Vec<ObjectId> = self.selection.iter().copied().collect();
        ids.sort_by_key(|i| i.number());
        results::IdCount {
            count: ids.len(),
            ids,
        }
    }

    fn check_selectable(&self, ids: &[ObjectId], adding: bool) -> Result<(), CommandError> {
        for &id in ids {
            if !self.doc.contains(id) {
                return Err(CommandError::not_found(id));
            }
            if !selectable(id) {
                return Err(CommandError::wrong_kind(id, "brush, mesh, entity or group"));
            }
            if adding && self.doc.effective_hidden(id) {
                return Err(CommandError::invalid_arg("ids", format!("{id} is hidden"))
                    .with_hint("show it first with object.set_hidden or object.show_all"));
            }
        }
        Ok(())
    }

    fn selectable_ids(&self, kind: Option<editor_core::ObjectKind>) -> BTreeSet<ObjectId> {
        let hidden = self.doc.hidden_ids();
        let locked = self.doc.locked_ids();
        self.doc
            .all_ids()
            .into_iter()
            .filter(|&id| selectable(id) && kind.is_none_or(|k| id.kind() == k))
            .filter(|id| !hidden.contains(id) && !locked.contains(id))
            .collect()
    }

    fn fresh_document(&mut self, doc: Document, path: Option<PathBuf>) {
        self.feed.replaced = true;
        self.doc = doc;
        self.path = path;
        self.journal.clear();
        self.undo.clear();
        self.redo.clear();
        self.selection.clear();
        self.face_selection.clear();
        self.revision += 1;
        self.saved_revision = self.revision;
    }

    fn guard_replace(&self, discard: bool) -> Result<(), CommandError> {
        if self.open.is_some() {
            return Err(CommandError::TransactionOpen);
        }
        if self.is_dirty() && !discard {
            return Err(CommandError::UnsavedChanges);
        }
        Ok(())
    }

    fn session_op(
        &mut self,
        op: &str,
        args: Value,
        actor: &Actor,
    ) -> Result<Success, CommandError> {
        match op {
            "select.get" => {
                let NoArgs {} = parse_args(args)?;
                Ok(self.success(self.selection_result(), Changes::default(), vec![]))
            }
            "select.set" | "select.add" | "select.remove" => {
                let SelectIds { ids } = parse_args(args)?;
                self.check_selectable(&ids, op != "select.remove")?;
                match op {
                    "select.set" => self.selection = ids.into_iter().collect(),
                    "select.add" => self.selection.extend(ids),
                    _ => {
                        for id in ids {
                            self.selection.remove(&id);
                        }
                    }
                }
                Ok(self.success(self.selection_result(), Changes::default(), vec![]))
            }
            "select.faces.get" => {
                let NoArgs {} = parse_args(args)?;
                Ok(self.success(self.face_selection_result(), Changes::default(), vec![]))
            }
            "select.faces.set" | "select.faces.add" | "select.faces.remove" => {
                let SelectFaces { faces } = parse_args(args)?;
                let picked = self.expand_faces(&faces, op != "select.faces.remove")?;
                match op {
                    "select.faces.set" => self.face_selection = picked,
                    "select.faces.add" => self.face_selection.extend(picked),
                    _ => self.face_selection.retain(|f| !picked.contains(f)),
                }
                Ok(self.success(self.face_selection_result(), Changes::default(), vec![]))
            }
            "select.faces.clear" => {
                let NoArgs {} = parse_args(args)?;
                self.face_selection.clear();
                Ok(self.success(self.face_selection_result(), Changes::default(), vec![]))
            }
            "select.clear" => {
                let NoArgs {} = parse_args(args)?;
                self.selection.clear();
                Ok(self.success(self.selection_result(), Changes::default(), vec![]))
            }
            "select.all" => {
                let SelectKind { kind } = parse_args(args)?;
                self.selection = self.selectable_ids(kind);
                Ok(self.success(self.selection_result(), Changes::default(), vec![]))
            }
            "select.invert" => {
                let SelectKind { kind } = parse_args(args)?;
                let universe = self.selectable_ids(kind);
                self.selection = universe.difference(&self.selection).copied().collect();
                Ok(self.success(self.selection_result(), Changes::default(), vec![]))
            }
            "file.info" => {
                let NoArgs {} = parse_args(args)?;
                Ok(self.success(
                    results::FileInfoResult {
                        path: self.path.as_ref().map(|p| p.display().to_string()),
                        name: self.doc.name.clone(),
                        dirty: self.is_dirty(),
                        revision: self.revision,
                    },
                    Changes::default(),
                    vec![],
                ))
            }
            "file.save" => {
                let Save { path, overwrite } = parse_args(args)?;
                if self.open.is_some() {
                    return Err(CommandError::TransactionOpen);
                }
                let target = match path {
                    Some(p) => PathBuf::from(p),
                    None => self.path.clone().ok_or_else(|| {
                        CommandError::invalid_arg("path", "the document has no file yet")
                            .with_hint("pass path to save it somewhere")
                    })?,
                };
                if !overwrite && self.path.as_ref() != Some(&target) && target.exists() {
                    return Err(CommandError::FileExists {
                        path: target.display().to_string(),
                    });
                }
                create_parent_dirs(&target)?;
                if self.path.is_none()
                    && self.doc.name == "untitled"
                    && let Some(name) = stem_name(&target)
                {
                    self.doc.name = name;
                }
                let rebased = self.rebase_textures(&target);
                if let Err(e) = editor_fmt::save_file(&target, &self.doc).map_err(file_error) {
                    if let Some((old_dir, new_dir)) = rebased {
                        self.rebase_textures_between(&new_dir, &old_dir);
                    }
                    return Err(e);
                }
                self.saved_revision = self.revision;
                self.path = Some(target.clone());
                Ok(self.success(
                    results::Saved {
                        path: target.display().to_string(),
                    },
                    Changes::default(),
                    vec![],
                ))
            }
            "file.new" => {
                let New {
                    name,
                    path,
                    discard,
                } = parse_args(args)?;
                self.guard_replace(discard)?;
                let path = path.map(PathBuf::from);
                if let Some(p) = &path
                    && p.exists()
                {
                    return Err(CommandError::FileExists {
                        path: p.display().to_string(),
                    });
                }
                let name = name
                    .or_else(|| path.as_deref().and_then(stem_name))
                    .unwrap_or_else(|| "untitled".into());
                self.fresh_document(Document::new(name), path);
                Ok(self.success(results::Empty {}, Changes::default(), vec![]))
            }
            "file.open" => {
                let Open { path, discard } = parse_args(args)?;
                self.guard_replace(discard)?;
                let path = PathBuf::from(path);
                let doc = editor_fmt::load_file(&path).map_err(file_error)?;
                self.fresh_document(doc, Some(path));
                Ok(self.success(results::Empty {}, Changes::default(), vec![]))
            }
            _ => self.history_op(op, args, actor),
        }
    }

    fn history_op(
        &mut self,
        op: &str,
        args: Value,
        actor: &Actor,
    ) -> Result<Success, CommandError> {
        match op {
            "history.begin" => {
                let Begin { label } = parse_args(args)?;
                if self.open.is_some() {
                    return Err(CommandError::TransactionOpen);
                }
                self.open = Some(OpenTxn {
                    label: label.unwrap_or_else(|| "transaction".into()),
                    actor: actor.clone(),
                    start: savepoint(&self.doc, &self.journal),
                    start_revision: self.revision,
                });
                Ok(self.success(results::Empty {}, Changes::default(), vec![]))
            }
            "history.commit" => {
                let NoArgs {} = parse_args(args)?;
                let txn = self.open.take().ok_or(CommandError::NoTransaction)?;
                let entries = collapse(&self.doc, &self.journal);
                self.journal.clear();
                let changes = changes_of(&entries, false);
                if !entries.is_empty() {
                    self.redo.clear();
                    self.undo.push(HistoryEntry {
                        label: txn.label,
                        client: txn.actor.name,
                        conn: txn.actor.conn,
                        merge_key: None,
                        revision: self.revision,
                        changes: entries,
                    });
                }
                Ok(self.success(results::Empty {}, changes, vec![]))
            }
            "history.rollback" => {
                let NoArgs {} = parse_args(args)?;
                let changes = self.rollback_open()?;
                Ok(self.success(results::Empty {}, changes, vec![]))
            }
            "history.undo" | "history.redo" => {
                let NoArgs {} = parse_args(args)?;
                if self.open.is_some() {
                    return Err(CommandError::TransactionOpen);
                }
                let undoing = op == "history.undo";
                let stack = if undoing {
                    &mut self.undo
                } else {
                    &mut self.redo
                };
                let Some(entry) = stack.pop() else {
                    return Err(CommandError::invalid_arg(
                        "op",
                        format!("nothing to {}", if undoing { "undo" } else { "redo" }),
                    ));
                };
                apply(&mut self.doc, &entry.changes, undoing);
                self.feed.ids.extend(entry.changes.iter().map(|c| c.id));
                let changes = changes_of(&entry.changes, undoing);
                let label = entry.label.clone();
                if undoing {
                    self.redo.push(entry);
                } else {
                    self.undo.push(entry);
                }
                self.revision += 1;
                Ok(self.success(results::Stepped { label }, changes, vec![]))
            }
            "history.list" => {
                let List { limit } = parse_args(args)?;
                let entries: Vec<results::HistoryRow> = self
                    .undo
                    .iter()
                    .rev()
                    .take(limit.unwrap_or(50))
                    .map(|e| {
                        let c = changes_of(&e.changes, false);
                        results::HistoryRow {
                            revision: e.revision,
                            label: e.label.clone(),
                            client: e.client.clone(),
                            created: c.created.len(),
                            modified: c.modified.len(),
                            deleted: c.deleted.len(),
                        }
                    })
                    .collect();
                Ok(self.success(
                    results::HistoryList {
                        entries,
                        can_undo: !self.undo.is_empty(),
                        can_redo: !self.redo.is_empty(),
                        open_transaction: self.open.as_ref().map(|t| t.label.clone()),
                    },
                    Changes::default(),
                    vec![],
                ))
            }
            _ => Err(CommandError::invalid_arg(
                "op",
                format!("unknown op `{op}`"),
            )),
        }
    }
}
