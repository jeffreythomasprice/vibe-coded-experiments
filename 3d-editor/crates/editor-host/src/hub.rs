use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

use editor_cmd::results::{
    ClipboardState, Discarded, Recovered, RecoveryList, SessionAttached, SessionList,
};
use editor_cmd::{
    Actor, Attach, ClipboardCopy, CommandError, Exec, Feed, FileState, HistoryState, OpInfo,
    RecoveryDiscard, RecoveryRestore, Request, Response, Session, parse_args,
};
use editor_core::{Document, ElementRef, ObjectId, Part};
use editor_proto::{
    ChangedEvent, ClientRow, ControlAction, ControlStatus, FileInfo, HistoryInfo, PROTOCOL,
    ServerMsg, SessionRow, SnapshotEvent, StopAck, TxnWire,
};
use editor_render::Renderer;
use serde_json::Value;

use crate::recovery::{
    self, Job, RecoverySettings, SlotRecovery, autosave_files, backup_path, key_of, sidecar_path,
    sidecar_text, valid_id,
};
use crate::{HostReply, Policy, shot};

pub type ConnId = u64;

pub struct Handled {
    pub events: Vec<(ConnId, ServerMsg)>,
    pub reply: HostReply,
}

impl Handled {
    fn error(e: &CommandError) -> Self {
        Self {
            events: Vec::new(),
            reply: HostReply {
                response: Response::error(e),
                png: None,
            },
        }
    }
}

struct Slot {
    session: Session,
    conns: BTreeSet<ConnId>,
    rec: SlotRecovery,
}

struct Conn {
    name: String,
    policy: Policy,
    session: Option<u64>,
    cwd: Option<PathBuf>,
}

#[derive(Default)]
struct Clipboard {
    text: Option<String>,
    count: usize,
    seq: u64,
}

impl Clipboard {
    fn state(&self) -> ClipboardState {
        ClipboardState {
            text: self.text.clone(),
            count: self.count,
            seq: self.seq,
        }
    }

    fn fill(&self, request: &mut Request) {
        let Some(text) = &self.text else { return };
        let is_paste = |op: &str| matches!(op, "clipboard.paste" | "clipboard_paste");
        let fill = |args: &mut Value| {
            if args.is_null() {
                *args = Value::Object(Default::default());
            }
            if let Value::Object(map) = args
                && !map.contains_key("text")
            {
                map.insert("text".into(), Value::String(text.clone()));
            }
        };
        if is_paste(&request.op) {
            fill(&mut request.args);
        } else if request.op == "batch"
            && let Some(Value::Array(items)) = request.args.get_mut("commands")
        {
            for item in items {
                if item.get("op").and_then(Value::as_str).is_some_and(is_paste) {
                    if item.get("args").is_none() {
                        item["args"] = Value::Null;
                    }
                    fill(&mut item["args"]);
                }
            }
        }
    }
}

const PATH_OPS: &[&str] = &[
    "session.attach",
    "file.open",
    "file.new",
    "file.save",
    "view.screenshot",
];

const TEXTURE_OPS: &[&str] = &["material.create", "material.set"];

fn resolve_paths(request: &mut Request, cwd: &Path) {
    if TEXTURE_OPS.contains(&request.op.as_str())
        && let Some(Value::Object(texture)) = request.args.get_mut("texture")
        && texture.get("source").and_then(Value::as_str) == Some("file")
        && let Some(Value::String(path)) = texture.get_mut("path")
        && Path::new(path.as_str()).is_relative()
    {
        *path = cwd.join(path.as_str()).display().to_string();
    }
    if !PATH_OPS.contains(&request.op.as_str()) {
        return;
    }
    if let Some(Value::String(path)) = request.args.get_mut("path")
        && Path::new(path.as_str()).is_relative()
    {
        *path = cwd.join(path.as_str()).display().to_string();
    }
}

fn session_name(id: u64) -> String {
    format!("s{id}")
}

fn parse_session_id(text: &str) -> Option<u64> {
    text.strip_prefix('s')?.parse().ok()
}

fn canonical(path: &Path) -> PathBuf {
    if let Ok(p) = path.canonicalize() {
        return p;
    }
    match (path.parent(), path.file_name()) {
        (Some(dir), Some(name)) => {
            let dir = if dir.as_os_str().is_empty() {
                Path::new(".")
            } else {
                dir
            };
            dir.canonicalize()
                .map_or_else(|_| path.to_path_buf(), |d| d.join(name))
        }
        _ => path.to_path_buf(),
    }
}

fn stem_name(path: &Path) -> String {
    path.file_name()
        .map(|n| {
            n.to_string_lossy()
                .trim_end_matches(".scene.toml")
                .to_string()
        })
        .unwrap_or_default()
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

fn file_info(f: FileState) -> FileInfo {
    FileInfo {
        path: f.path,
        name: f.name,
        dirty: f.dirty,
    }
}

fn history_info(h: HistoryState) -> HistoryInfo {
    HistoryInfo {
        can_undo: h.can_undo,
        can_redo: h.can_redo,
        transaction: h.transaction.map(|t| TxnWire {
            holder: t.holder,
            conn: t.conn,
            label: t.label,
        }),
    }
}

fn ids_sorted(ids: &std::collections::BTreeSet<ObjectId>) -> Vec<ObjectId> {
    let mut v: Vec<ObjectId> = ids.iter().copied().collect();
    v.sort_by_key(|i| i.number());
    v
}

fn selection_of(session: &Session) -> Vec<ObjectId> {
    ids_sorted(session.selection())
}

fn face_selection_of(session: &Session) -> Vec<ElementRef> {
    session
        .face_selection()
        .iter()
        .map(|&(object, face)| ElementRef {
            object,
            part: Part::Face(face),
        })
        .collect()
}

/// All sessions, connections and the shared renderer. Does no I/O: callers deliver the returned messages.
pub struct Hub {
    sessions: BTreeMap<u64, Slot>,
    conns: HashMap<ConnId, Conn>,
    renderer: Option<Renderer>,
    focused: Option<u64>,
    clipboard: Clipboard,
    recovery: Option<RecoverySettings>,
    run: String,
    orphans: Vec<PathBuf>,
    next_conn: ConnId,
    next_session: u64,
}

impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

impl Hub {
    pub fn new() -> Self {
        Self {
            sessions: BTreeMap::new(),
            conns: HashMap::new(),
            renderer: None,
            focused: None,
            clipboard: Clipboard::default(),
            recovery: None,
            run: format!(
                "{:x}-p{}",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos()),
                std::process::id()
            ),
            orphans: Vec::new(),
            next_conn: 1,
            next_session: 1,
        }
    }

    /// A hub that autosaves dirty sessions and snapshots backups into `settings.dir`; callers drive `recovery_due`.
    pub fn with_recovery(settings: RecoverySettings) -> Self {
        Self {
            recovery: Some(settings),
            ..Self::new()
        }
    }

    pub fn connect(
        &mut self,
        client: &str,
        protocol: u32,
        policy: Policy,
    ) -> Result<ConnId, CommandError> {
        if protocol != PROTOCOL {
            return Err(CommandError::Protocol {
                message: format!("client speaks protocol {protocol}, the server speaks {PROTOCOL}"),
            });
        }
        let id = self.next_conn;
        self.next_conn += 1;
        self.conns.insert(
            id,
            Conn {
                name: client.to_string(),
                policy,
                session: None,
                cwd: None,
            },
        );
        Ok(id)
    }

    /// Relative `path` arguments from this connection resolve against `cwd`.
    pub fn set_cwd(&mut self, conn: ConnId, cwd: Option<PathBuf>) {
        if let Some(c) = self.conns.get_mut(&conn) {
            c.cwd = cwd;
        }
    }

    pub fn disconnect(&mut self, conn: ConnId) -> Vec<(ConnId, ServerMsg)> {
        let Some(c) = self.conns.remove(&conn) else {
            return Vec::new();
        };
        let Some(sid) = c.session else {
            return Vec::new();
        };
        self.detach(conn, sid)
    }

    pub fn conn_count(&self) -> usize {
        self.conns.len()
    }

    pub fn dirty_sessions(&self) -> usize {
        self.sessions
            .values()
            .filter(|s| s.session.is_dirty())
            .count()
    }

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn session(&self, conn: ConnId) -> Option<&Session> {
        let sid = self.conns.get(&conn)?.session?;
        self.sessions.get(&sid).map(|s| &s.session)
    }

    fn detach(&mut self, conn: ConnId, sid: u64) -> Vec<(ConnId, ServerMsg)> {
        let mut events = Vec::new();
        let Some(slot) = self.sessions.get_mut(&sid) else {
            return events;
        };
        slot.conns.remove(&conn);
        if slot.session.abandon(conn) {
            events = self.events_for(sid);
        }
        let slot = &self.sessions[&sid];
        if slot.conns.is_empty()
            && !slot.session.is_dirty()
            && let Some(gone) = self.sessions.remove(&sid)
        {
            self.orphans
                .extend(gone.rec.own.iter().flat_map(|p| autosave_files(p)));
            self.orphans
                .extend(gone.rec.superseded.iter().flat_map(|p| autosave_files(p)));
        }
        if self.focused == Some(sid) && !self.sessions.contains_key(&sid) {
            self.focused = self.sessions.keys().next_back().copied();
        }
        events
    }

    fn snapshot(&self, sid: u64) -> SnapshotEvent {
        let session = &self.sessions[&sid].session;
        SnapshotEvent {
            session: session_name(sid),
            revision: session.revision(),
            document_toml: editor_fmt::write(session.document()),
            selection: selection_of(session),
            face_selection: face_selection_of(session),
            file: file_info(session.file_state()),
            history: history_info(session.history_state()),
        }
    }

    fn events_for(&mut self, sid: u64) -> Vec<(ConnId, ServerMsg)> {
        let Some(slot) = self.sessions.get_mut(&sid) else {
            return Vec::new();
        };
        let feed: Feed = slot.session.take_feed();
        if feed.is_empty() {
            return Vec::new();
        }
        let targets: Vec<ConnId> = slot.conns.iter().copied().collect();
        let msg = if feed.replaced {
            ServerMsg::Snapshot(self.snapshot(sid))
        } else {
            let session = &self.sessions[&sid].session;
            let doc: &Document = session.document();
            let mut upsert = Vec::new();
            let mut deleted = Vec::new();
            for id in ids_sorted(&feed.ids) {
                match doc.get(id) {
                    Some(object) => upsert.push(object),
                    None => deleted.push(id),
                }
            }
            ServerMsg::Changed(ChangedEvent {
                session: session_name(sid),
                revision: session.revision(),
                upsert,
                deleted,
                selection: feed.selection.then(|| selection_of(session)),
                face_selection: feed.face_selection.then(|| face_selection_of(session)),
                file: feed.file.then(|| file_info(session.file_state())),
                history: feed.history.then(|| history_info(session.history_state())),
            })
        };
        targets.into_iter().map(|c| (c, msg.clone())).collect()
    }

    fn client_rows(&self, sid: u64) -> Vec<ClientRow> {
        self.sessions[&sid]
            .conns
            .iter()
            .filter_map(|c| {
                self.conns.get(c).map(|conn| ClientRow {
                    conn: *c,
                    client: conn.name.clone(),
                })
            })
            .collect()
    }

    fn row(&self, sid: u64) -> SessionRow {
        let session = &self.sessions[&sid].session;
        let f = session.file_state();
        SessionRow {
            id: session_name(sid),
            path: f.path,
            name: f.name,
            dirty: f.dirty,
            revision: session.revision(),
            focused: self.focused == Some(sid),
            clients: self.client_rows(sid),
        }
    }

    fn path_owner(&self, path: &Path, except: Option<u64>) -> Option<u64> {
        let target = canonical(path);
        self.sessions.iter().find_map(|(sid, slot)| {
            if Some(*sid) == except {
                return None;
            }
            let p = slot.session.path()?;
            (canonical(p) == target).then_some(*sid)
        })
    }

    pub fn handle(&mut self, conn: ConnId, request: Request) -> Handled {
        let Some(c) = self.conns.get(&conn) else {
            return Handled::error(&CommandError::Protocol {
                message: "unknown connection".into(),
            });
        };
        if let Err(e) = c.policy.check(&request) {
            return Handled::error(&e);
        }
        let name = c.name.clone();
        let attached = c.session;
        let cwd = c.cwd.clone();
        let mut request = request;
        let info = OpInfo::find(&request.op);
        if let Some(info) = info {
            request.op = info.name.to_string();
        }
        if let Some(cwd) = &cwd {
            resolve_paths(&mut request, cwd);
        }
        self.clipboard.fill(&mut request);
        if info.is_some_and(|o| o.exec == Exec::Server) {
            return self.server_op(conn, &request);
        }
        let Some(sid) = attached.filter(|s| self.sessions.contains_key(s)) else {
            return Handled::error(
                &CommandError::invalid_arg("op", "this connection has no session")
                    .with_hint("call session.attach first (path to open a file, or no args for the current session)"),
            );
        };
        if let Err(e) = self.check_file_in_use(sid, &request) {
            return Handled::error(&e);
        }
        let actor = Actor::new(conn, name);
        let reply = if info.is_some_and(|o| o.exec == Exec::Client) {
            self.client_op(sid, request)
        } else {
            HostReply {
                response: self
                    .sessions
                    .get_mut(&sid)
                    .expect("checked")
                    .session
                    .execute_as(request, &actor),
                png: None,
            }
        };
        let events = self.events_for(sid);
        Handled { events, reply }
    }

    fn check_file_in_use(&self, sid: u64, request: &Request) -> Result<(), CommandError> {
        if !matches!(request.op.as_str(), "file.open" | "file.new" | "file.save") {
            return Ok(());
        }
        let Some(path) = request.args.get("path").and_then(Value::as_str) else {
            return Ok(());
        };
        match self.path_owner(Path::new(path), Some(sid)) {
            Some(owner) => Err(CommandError::FileInUse {
                path: path.to_string(),
                session: session_name(owner),
            }),
            None => Ok(()),
        }
    }

    fn client_op(&mut self, sid: u64, request: Request) -> HostReply {
        let session = &self.sessions[&sid].session;
        match shot::screenshot(&mut self.renderer, session, request) {
            Ok(reply) => reply,
            Err(e) => HostReply {
                response: Response::error(&e),
                png: None,
            },
        }
    }

    fn ok<R: serde::Serialize>(
        &self,
        result: R,
        events: Vec<(ConnId, ServerMsg)>,
        revision: u64,
    ) -> Handled {
        Handled {
            events,
            reply: HostReply {
                response: Response::Success(editor_cmd::Success {
                    ok: true,
                    revision,
                    result: serde_json::to_value(result).expect("results serialize"),
                    changes: editor_cmd::Changes::default(),
                    warnings: vec![],
                }),
                png: None,
            },
        }
    }

    fn server_op(&mut self, conn: ConnId, request: &Request) -> Handled {
        let result = match request.op.as_str() {
            "session.list" => Ok(Handled::from_rows(self)),
            "session.info" => self.session_info(conn),
            "session.attach" => self.attach(conn, request),
            "clipboard.copy" => self.clipboard_copy(conn, request),
            "recovery.list" => self.recovery_list(conn),
            "recovery.restore" => self.recovery_restore(conn, request),
            "recovery.discard" => self.recovery_discard(conn, request),
            "clipboard.get" => {
                let revision = self.conn_session(conn).map_or(0, Session::revision);
                Ok(self.ok(self.clipboard.state(), Vec::new(), revision))
            }
            other => Err(CommandError::invalid_arg(
                "op",
                format!("unknown op `{other}`"),
            )),
        };
        match result {
            Ok(h) => h,
            Err(e) => Handled::error(&e),
        }
    }

    /// Work for the recovery directory: autosaves of dirty sessions, interval backups, and cleanup of what a save or close made stale. `force` ignores the autosave interval (a stopping server).
    pub fn recovery_due(&mut self, now: Instant, force: bool) -> Job {
        let mut job = Job::default();
        let Some(settings) = self.recovery.clone() else {
            return job;
        };
        job.removes.append(&mut self.orphans);
        let run = self.run.clone();
        for (&sid, slot) in &mut self.sessions {
            let session = &slot.session;
            let rec = &mut slot.rec;
            let key = key_of(session.path());
            if rec.key != key {
                if let Some(own) = rec.own.take() {
                    job.removes.extend(autosave_files(&own));
                }
                rec.autosaved_rev = None;
                rec.key = key.clone();
            }
            let revision = session.revision();
            let busy = session.has_open_transaction();
            let mut text: Option<String> = None;
            if !session.is_dirty() {
                if let Some(own) = rec.own.take() {
                    job.removes.extend(autosave_files(&own));
                }
                rec.autosaved_rev = None;
                for old in rec.superseded.drain(..) {
                    job.removes.extend(autosave_files(&old));
                }
            } else if !busy
                && let Some(every) = settings.autosave
                && rec.autosaved_rev != Some(revision)
                && (force || now.duration_since(rec.last_autosave) >= every)
            {
                let own = settings.autosave_path(&format!("{key}.{run}-s{sid}"));
                let body = text.get_or_insert_with(|| editor_fmt::write(session.document()));
                job.writes.push((own.clone(), body.clone()));
                job.writes.push((
                    sidecar_path(&own),
                    sidecar_text(session.path(), &session.document().name, revision),
                ));
                rec.own = Some(own);
                rec.autosaved_rev = Some(revision);
                rec.last_autosave = now;
                for old in rec.superseded.drain(..) {
                    job.removes.extend(autosave_files(&old));
                }
            }
            if !busy
                && let Some(every) = settings.backup
                && rec.backup_rev != Some(revision)
                && now.duration_since(rec.last_backup) >= every
            {
                let body = text.get_or_insert_with(|| editor_fmt::write(session.document()));
                job.writes.push((
                    backup_path(&settings, &key, recovery::unix_now()),
                    body.clone(),
                ));
                job.prune.push((settings.backup_dir(&key), settings.keep));
                rec.backup_rev = Some(revision);
                rec.last_backup = now;
            }
        }
        job
    }

    fn live_ids(&self) -> BTreeSet<String> {
        self.sessions
            .values()
            .flat_map(|s| s.rec.own.iter().chain(&s.rec.superseded))
            .map(|p| recovery::id_of(p))
            .collect()
    }

    fn recovery_settings(&self) -> Result<RecoverySettings, CommandError> {
        self.recovery.clone().ok_or_else(|| {
            CommandError::invalid_arg("op", "this server has no recovery directory configured")
        })
    }

    fn checked_autosave(&self, id: &str) -> Result<PathBuf, CommandError> {
        let settings = self.recovery_settings()?;
        if !valid_id(id) {
            return Err(CommandError::invalid_arg(
                "id",
                "not an autosave id; take one from recovery.list",
            ));
        }
        if self.live_ids().contains(id) {
            return Err(CommandError::invalid_arg(
                "id",
                "that autosave belongs to a running session",
            ));
        }
        let path = settings.autosave_path(id);
        if !path.exists() {
            return Err(CommandError::not_found(id));
        }
        Ok(path)
    }

    fn recovery_list(&self, conn: ConnId) -> Result<Handled, CommandError> {
        let settings = self.recovery_settings()?;
        let entries = recovery::list(&settings, &self.live_ids());
        let revision = self.conn_session(conn).map_or(0, Session::revision);
        Ok(self.ok(RecoveryList { entries }, Vec::new(), revision))
    }

    fn recovery_restore(
        &mut self,
        conn: ConnId,
        request: &Request,
    ) -> Result<Handled, CommandError> {
        let RecoveryRestore { id, discard } = parse_args(request.args.clone())?;
        let path = self.checked_autosave(&id)?;
        let sid = self
            .conns
            .get(&conn)
            .and_then(|c| c.session)
            .ok_or_else(|| {
                CommandError::invalid_arg("op", "this connection has no session")
                    .with_hint("call session.attach first")
            })?;
        let doc = editor_fmt::load_file(&path).map_err(file_error)?;
        let slot = self
            .sessions
            .get_mut(&sid)
            .expect("attached session exists");
        slot.session.recover(doc, discard)?;
        slot.rec.superseded.push(path);
        slot.rec.autosaved_rev = None;
        let file = slot.session.path().map(|p| p.display().to_string());
        let revision = slot.session.revision();
        let events = self.events_for(sid);
        Ok(self.ok(Recovered { id, path: file }, events, revision))
    }

    fn recovery_discard(
        &mut self,
        conn: ConnId,
        request: &Request,
    ) -> Result<Handled, CommandError> {
        let RecoveryDiscard { id } = parse_args(request.args.clone())?;
        let path = self.checked_autosave(&id)?;
        for file in autosave_files(&path) {
            let _ = std::fs::remove_file(file);
        }
        let revision = self.conn_session(conn).map_or(0, Session::revision);
        Ok(self.ok(Discarded { id }, Vec::new(), revision))
    }

    fn conn_session(&self, conn: ConnId) -> Option<&Session> {
        let sid = self.conns.get(&conn)?.session?;
        self.sessions.get(&sid).map(|s| &s.session)
    }

    fn clipboard_copy(&mut self, conn: ConnId, request: &Request) -> Result<Handled, CommandError> {
        let ClipboardCopy { ids } = parse_args(request.args.clone())?;
        let session = self.conn_session(conn).ok_or_else(|| {
            CommandError::invalid_arg("op", "this connection has no session")
                .with_hint("call session.attach first")
        })?;
        let copied =
            editor_cmd::copy_text(session.document(), session.scene_dir().as_deref(), &ids)?;
        let revision = session.revision();
        self.clipboard = Clipboard {
            text: Some(copied.text),
            count: copied.count,
            seq: self.clipboard.seq + 1,
        };
        Ok(self.ok(self.clipboard.state(), Vec::new(), revision))
    }

    fn session_info(&self, conn: ConnId) -> Result<Handled, CommandError> {
        let sid = self
            .conns
            .get(&conn)
            .and_then(|c| c.session)
            .ok_or_else(|| CommandError::invalid_arg("op", "this connection has no session"))?;
        let revision = self.sessions[&sid].session.revision();
        Ok(self.ok(self.row(sid), Vec::new(), revision))
    }

    fn new_slot(&mut self, session: Session) -> u64 {
        let sid = self.next_session;
        self.next_session += 1;
        self.sessions.insert(
            sid,
            Slot {
                rec: SlotRecovery::new(session.revision()),
                session,
                conns: BTreeSet::new(),
            },
        );
        sid
    }

    fn load_or_create(&mut self, path: &str, create: bool) -> Result<(u64, bool), CommandError> {
        let path = Path::new(path);
        if let Some(sid) = self.path_owner(path, None) {
            return Ok((sid, false));
        }
        let session = if path.exists() {
            let canon = canonical(path);
            Session::with_path(editor_fmt::load_file(&canon).map_err(file_error)?, canon)
        } else if create {
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).map_err(|e| CommandError::Io {
                    message: format!("{}: {e}", dir.display()),
                })?;
            }
            let doc = Document::new(stem_name(path));
            editor_fmt::save_file(path, &doc).map_err(file_error)?;
            Session::with_path(doc, canonical(path))
        } else {
            return Err(CommandError::Io {
                message: format!(
                    "{} does not exist (pass create: true to start a new scene there)",
                    path.display()
                ),
            });
        };
        Ok((self.new_slot(session), true))
    }

    fn attach(&mut self, conn: ConnId, request: &Request) -> Result<Handled, CommandError> {
        let Attach { id, path, create } = parse_args(request.args.clone())?;
        let (sid, created) = match (id, path) {
            (Some(_), Some(_)) => {
                return Err(CommandError::invalid_arg("id", "pass id or path, not both"));
            }
            (Some(id), None) => {
                let sid = parse_session_id(&id)
                    .filter(|s| self.sessions.contains_key(s))
                    .ok_or_else(|| CommandError::not_found(&id))?;
                (sid, false)
            }
            (None, Some(path)) => self.load_or_create(&path, create)?,
            (None, None) => match self.focused.filter(|s| self.sessions.contains_key(s)) {
                Some(sid) => (sid, false),
                None => (self.new_slot(Session::new(Document::new("untitled"))), true),
            },
        };
        let previous = self.conns.get(&conn).and_then(|c| c.session);
        let mut events = Vec::new();
        if let Some(prev) = previous
            && prev != sid
        {
            events = self.detach(conn, prev);
        }
        let name = {
            let c = self.conns.get_mut(&conn).expect("checked by handle");
            c.session = Some(sid);
            c.name.clone()
        };
        self.sessions
            .get_mut(&sid)
            .expect("exists")
            .conns
            .insert(conn);
        if name == "gui" || self.focused.is_none() {
            self.focused = Some(sid);
        }
        events.push((conn, ServerMsg::Snapshot(self.snapshot(sid))));
        let row = self.row(sid);
        let attached = SessionAttached {
            id: row.id,
            path: row.path,
            name: row.name,
            dirty: row.dirty,
            revision: row.revision,
            focused: row.focused,
            clients: row.clients,
            created,
        };
        let revision = self.sessions[&sid].session.revision();
        Ok(self.ok(attached, events, revision))
    }

    /// Server-level control for the `editor-cli server` commands.
    pub fn control(
        &mut self,
        conn: ConnId,
        action: ControlAction,
        force: bool,
    ) -> (Response, bool) {
        let can_stop = self
            .conns
            .get(&conn)
            .is_some_and(|c| c.policy.allows(editor_cmd::Safety::Destructive));
        let success = |result: serde_json::Value| {
            Response::Success(editor_cmd::Success {
                ok: true,
                revision: 0,
                result,
                changes: editor_cmd::Changes::default(),
                warnings: vec![],
            })
        };
        match action {
            ControlAction::Status => {
                let status = ControlStatus {
                    protocol: PROTOCOL,
                    connections: self.conns.len(),
                    dirty_sessions: self.dirty_sessions(),
                    sessions: self.sessions.keys().map(|s| self.row(*s)).collect(),
                };
                (
                    success(serde_json::to_value(status).expect("status serializes")),
                    false,
                )
            }
            ControlAction::Stop if !can_stop => (
                Response::error(&CommandError::NotPermitted {
                    op: "server.stop".into(),
                    needs: "destructive".into(),
                }),
                false,
            ),
            ControlAction::Stop if self.dirty_sessions() > 0 && !force => {
                (Response::error(&CommandError::UnsavedChanges), false)
            }
            ControlAction::Stop => (
                success(serde_json::to_value(StopAck { stopping: true }).expect("ack serializes")),
                true,
            ),
        }
    }
}

impl Handled {
    fn from_rows(hub: &Hub) -> Self {
        let sessions = hub.sessions.keys().map(|s| hub.row(*s)).collect();
        let focused = hub.focused.map(session_name);
        hub.ok(SessionList { sessions, focused }, Vec::new(), 0)
    }
}
