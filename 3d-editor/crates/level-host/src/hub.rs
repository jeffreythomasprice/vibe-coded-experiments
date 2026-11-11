use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use level_cmd::results::{SessionAttached, SessionList};
use level_cmd::{
    Actor, Attach, CommandError, Exec, Feed, FileState, HistoryState, OpInfo, Request, Response,
    Session, parse_args,
};
use level_core::{Document, ObjectId};
use level_proto::{
    ChangedEvent, ClientRow, ControlAction, ControlStatus, FileInfo, HistoryInfo, PROTOCOL,
    ServerMsg, SessionRow, SnapshotEvent, StopAck, TxnWire,
};
use level_render::Renderer;
use serde_json::Value;

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
}

struct Conn {
    name: String,
    policy: Policy,
    session: Option<u64>,
    cwd: Option<PathBuf>,
}

const PATH_OPS: &[&str] = &[
    "session.attach",
    "file.open",
    "file.new",
    "file.save",
    "view.screenshot",
];

fn resolve_paths(request: &mut Request, cwd: &Path) {
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
            dir.canonicalize().map_or_else(|_| path.to_path_buf(), |d| d.join(name))
        }
        _ => path.to_path_buf(),
    }
}

fn stem_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().trim_end_matches(".level.toml").to_string())
        .unwrap_or_default()
}

fn file_error(e: level_fmt::FileError) -> CommandError {
    match e {
        level_fmt::FileError::Io { .. } => CommandError::Io {
            message: e.to_string(),
        },
        level_fmt::FileError::Load { .. } => CommandError::Format {
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

/// All sessions, connections and the shared renderer. Does no I/O: callers deliver the returned messages.
pub struct Hub {
    sessions: BTreeMap<u64, Slot>,
    conns: HashMap<ConnId, Conn>,
    renderer: Option<Renderer>,
    focused: Option<u64>,
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
            next_conn: 1,
            next_session: 1,
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
                message: format!(
                    "client speaks protocol {protocol}, the server speaks {PROTOCOL}"
                ),
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
        if slot.conns.is_empty() && !slot.session.is_dirty() {
            self.sessions.remove(&sid);
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
            document_toml: level_fmt::write(session.document()),
            selection: selection_of(session),
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
                response: self.sessions.get_mut(&sid).expect("checked").session.execute_as(request, &actor),
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
                response: Response::Success(level_cmd::Success {
                    ok: true,
                    revision,
                    result: serde_json::to_value(result).expect("results serialize"),
                    changes: level_cmd::Changes::default(),
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
            other => Err(CommandError::invalid_arg("op", format!("unknown op `{other}`"))),
        };
        match result {
            Ok(h) => h,
            Err(e) => Handled::error(&e),
        }
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
            Session::with_path(level_fmt::load_file(&canon).map_err(file_error)?, canon)
        } else if create {
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).map_err(|e| CommandError::Io {
                    message: format!("{}: {e}", dir.display()),
                })?;
            }
            let doc = Document::new(stem_name(path));
            level_fmt::save_file(path, &doc).map_err(file_error)?;
            Session::with_path(doc, canonical(path))
        } else {
            return Err(CommandError::Io {
                message: format!(
                    "{} does not exist (pass create: true to start a new level there)",
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
                None => (
                    self.new_slot(Session::new(Document::new("untitled"))),
                    true,
                ),
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
        self.sessions.get_mut(&sid).expect("exists").conns.insert(conn);
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

    /// Server-level control for the `level-cli server` commands.
    pub fn control(&mut self, conn: ConnId, action: ControlAction, force: bool) -> (Response, bool) {
        let can_stop = self.conns.get(&conn).is_some_and(|c| c.policy.allows(level_cmd::Safety::Destructive));
        let success = |result: serde_json::Value| {
            Response::Success(level_cmd::Success {
                ok: true,
                revision: 0,
                result,
                changes: level_cmd::Changes::default(),
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
                (success(serde_json::to_value(status).expect("status serializes")), false)
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
