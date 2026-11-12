use std::path::Path;

use editor_cmd::args::Attach;
use editor_cmd::results::SessionAttached;
use editor_cmd::{ErrorResponse, Request};
use editor_proto::{Applied, Client, Config, Replica, WakeFn};

use crate::error::GameError;

pub enum Target<'a> {
    Focused,
    Path(&'a Path),
    Id(&'a str),
}

/// A read-only view of one editor session: the server's document, kept current by its change events.
pub struct Session {
    client: Client,
    pub replica: Replica,
}

fn refusal(error: Option<&ErrorResponse>) -> String {
    error
        .map_or("refused".to_string(), |e| {
            format!("{} {}", e.message, e.hint)
        })
        .trim()
        .to_string()
}

impl Session {
    pub fn connect(target: Target, default: &Path, wake: WakeFn) -> Result<Self, GameError> {
        let config = Config::load()?;
        let client = Client::connect_with(&config, "game", false, false, true, Some(wake))?;
        let mut session = Self {
            client,
            replica: Replica::default(),
        };
        let focused = matches!(target, Target::Focused);
        let attached = session.attach(target)?;
        if focused && attached.created {
            session.attach(Target::Path(default))?;
        }
        Ok(session)
    }

    fn attach(&mut self, target: Target) -> Result<SessionAttached, GameError> {
        let args = match target {
            Target::Focused => Attach {
                id: None,
                path: None,
                create: false,
            },
            Target::Path(p) => Attach {
                id: None,
                path: Some(p.to_string_lossy().into_owned()),
                create: false,
            },
            Target::Id(id) => Attach {
                id: Some(id.to_string()),
                path: None,
                create: false,
            },
        };
        let value = serde_json::to_value(args).map_err(|e| GameError::Refused(e.to_string()))?;
        let reply = self.client.request(Request::new("session.attach", value))?;
        if !reply.is_ok() {
            return Err(GameError::Refused(refusal(reply.error())));
        }
        let attached = reply.result::<SessionAttached>()?;
        self.pump()?;
        Ok(attached)
    }

    /// Applies queued server events.
    pub fn pump(&mut self) -> Result<Applied, GameError> {
        let mut total = Applied::default();
        while let Some(event) = self.client.try_event() {
            let applied = self.replica.apply(event)?;
            total.document |= applied.document;
            total.defs |= applied.defs;
        }
        Ok(total)
    }
}
