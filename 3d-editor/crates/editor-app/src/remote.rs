use std::collections::BTreeSet;
use std::path::Path;

use editor_cmd::args::Attach;
use editor_cmd::results::SessionAttached;
use editor_cmd::{CommandError, ErrorResponse, Request, Response, Success};
use editor_core::{Document, FaceId, ObjectId};
use editor_proto::{
    Client, ClientError, Config, ConfigError, FileInfo, HistoryInfo, ServerEvent, WakeFn,
};

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("{0}")]
    Refused(String),
    #[error("the server sent an unreadable document: {0}")]
    Document(#[from] editor_fmt::LoadError),
}

/// A local copy of the attached session, kept current by the server's change events.
pub struct Remote {
    client: Client,
    pub doc: Document,
    pub selection: BTreeSet<ObjectId>,
    pub face_selection: BTreeSet<(ObjectId, FaceId)>,
    pub revision: u64,
    pub file: FileInfo,
    pub history: HistoryInfo,
    pub session: String,
    pub closed: bool,
}

fn refusal(error: Option<&ErrorResponse>) -> String {
    error
        .map_or("refused".to_string(), |e| {
            format!("{} {}", e.message, e.hint)
        })
        .trim()
        .to_string()
}

impl Remote {
    pub fn connect(path: Option<&Path>, default: &Path, wake: WakeFn) -> Result<Self, RemoteError> {
        let config = Config::load()?;
        let client = Client::connect_with(&config, "gui", true, true, true, Some(wake))?;
        let mut remote = Self {
            client,
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
        };
        let attached = remote.attach(path)?;
        if path.is_none() && attached.created {
            remote.attach(Some(default))?;
        }
        Ok(remote)
    }

    /// Switches to the session for `path`, or to the focused session when `None`.
    pub fn attach(&mut self, path: Option<&Path>) -> Result<SessionAttached, RemoteError> {
        let args = Attach {
            id: None,
            path: path.map(|p| p.to_string_lossy().into_owned()),
            create: false,
        };
        let reply = self.client.request(Request::new(
            "session.attach",
            serde_json::to_value(args).map_err(|e| RemoteError::Refused(e.to_string()))?,
        ))?;
        if !reply.is_ok() {
            return Err(RemoteError::Refused(refusal(reply.error())));
        }
        let attached = reply.result::<SessionAttached>()?;
        self.pump()?;
        Ok(attached)
    }

    /// Sends a request and applies the events that arrived with it. Err is the error response.
    pub fn call(&mut self, request: Request) -> Result<Success, ErrorResponse> {
        let reply = match self.client.request(request) {
            Ok(r) => r,
            Err(e) => {
                self.closed = true;
                return Err(CommandError::from(e).to_response());
            }
        };
        let _ = self.pump();
        match reply.response {
            Response::Success(s) => Ok(s),
            Response::Error(e) => Err(e),
        }
    }

    /// Applies queued events; true if anything changed.
    pub fn pump(&mut self) -> Result<bool, RemoteError> {
        let mut changed = false;
        while let Some(event) = self.client.try_event() {
            changed = true;
            match event {
                ServerEvent::Snapshot(s) => {
                    self.doc = editor_fmt::read(&s.document_toml)?;
                    self.selection = s.selection.into_iter().collect();
                    self.face_selection = face_set(s.face_selection);
                    self.revision = s.revision;
                    self.file = s.file;
                    self.history = s.history;
                    self.session = s.session;
                }
                ServerEvent::Changed(c) => {
                    for object in c.upsert {
                        self.doc.insert(object);
                    }
                    for id in c.deleted {
                        self.doc.remove(id);
                    }
                    if let Some(selection) = c.selection {
                        self.selection = selection.into_iter().collect();
                    }
                    if let Some(faces) = c.face_selection {
                        self.face_selection = face_set(faces);
                    }
                    if let Some(file) = c.file {
                        self.file = file;
                    }
                    if let Some(history) = c.history {
                        self.history = history;
                    }
                    self.revision = c.revision;
                }
                ServerEvent::Closed => self.closed = true,
            }
        }
        Ok(changed)
    }

    /// The other client holding a transaction open, if any.
    pub fn busy_holder(&self) -> Option<(&str, &str)> {
        self.history
            .transaction
            .as_ref()
            .filter(|t| t.conn != self.client.conn())
            .map(|t| (t.holder.as_str(), t.label.as_str()))
    }
}

fn face_set(refs: Vec<editor_core::ElementRef>) -> BTreeSet<(ObjectId, FaceId)> {
    refs.into_iter()
        .filter_map(|r| match r.part {
            editor_core::Part::Face(f) => Some((r.object, f)),
            _ => None,
        })
        .collect()
}
