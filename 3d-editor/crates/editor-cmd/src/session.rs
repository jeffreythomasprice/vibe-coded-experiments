use std::collections::BTreeSet;
use std::path::PathBuf;

use editor_core::{Document, ObjectId};
use serde_json::Value;

use crate::commands::session_args::{Begin, List, New, Open, Save, SelectIds, SelectKind};
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

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
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
        let file = self.file_state();
        let history = self.history_state();
        let result = self.run(request, actor);
        self.feed.selection |= self.selection != selection;
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
            let mut ctx = Ctx::new(&mut self.doc, &mut self.journal, &self.selection);
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

    fn prune_selection(&mut self) {
        let doc = &self.doc;
        self.selection
            .retain(|id| doc.contains(*id) && !doc.effective_hidden(*id));
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
                editor_fmt::save_file(&target, &self.doc).map_err(file_error)?;
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
