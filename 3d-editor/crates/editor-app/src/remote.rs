use std::path::Path;

use editor_cmd::args::Attach;
use editor_cmd::component_defs::Catalog;
use editor_cmd::results::{ComponentDefsList, SessionAttached};
use editor_cmd::{CommandError, ErrorResponse, Request, Response, Success};
use editor_proto::{Client, ClientError, Config, ConfigError, Replica, ReplicaError, WakeFn};

#[derive(Debug, thiserror::Error)]
pub enum RemoteError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Client(#[from] ClientError),
    #[error("{0}")]
    Refused(String),
    #[error(transparent)]
    Replica(#[from] ReplicaError),
}

/// The attached session: a `Replica` kept current by the server's events, plus the resolved component definitions.
pub struct Remote {
    client: Client,
    replica: Replica,
    /// Resolved by the server, which reads definition files relative to the scene.
    pub catalog: Catalog,
    pub defs: ComponentDefsList,
    pub defs_generation: u64,
    defs_dirty: bool,
}

impl std::ops::Deref for Remote {
    type Target = Replica;

    fn deref(&self) -> &Replica {
        &self.replica
    }
}

impl std::ops::DerefMut for Remote {
    fn deref_mut(&mut self) -> &mut Replica {
        &mut self.replica
    }
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
            replica: Replica::default(),
            catalog: Catalog::default(),
            defs: ComponentDefsList {
                defs: Vec::new(),
                components: Vec::new(),
                problems: Vec::new(),
            },
            defs_generation: 0,
            defs_dirty: true,
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
            let applied = self.replica.apply(event)?;
            self.defs_dirty |= applied.defs;
        }
        Ok(changed)
    }

    /// Asks the server to resolve the definitions again (after a file changed on disk, or the scene moved).
    pub fn reload_defs(&mut self) {
        self.defs_dirty = true;
    }

    /// Refetches the resolved definitions when something they depend on changed.
    pub fn refresh_defs(&mut self) -> Result<(), RemoteError> {
        if !self.defs_dirty {
            return Ok(());
        }
        self.defs_dirty = false;
        let reply = self
            .client
            .request(Request::new("component_defs.list", serde_json::json!({})))?;
        if !reply.is_ok() {
            return Err(RemoteError::Refused(refusal(reply.error())));
        }
        let list = reply.result::<ComponentDefsList>()?;
        self.catalog =
            Catalog::from_components(list.components.iter().map(|c| (c.from, c.def.clone())));
        self.defs = list;
        self.defs_generation += 1;
        Ok(())
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
