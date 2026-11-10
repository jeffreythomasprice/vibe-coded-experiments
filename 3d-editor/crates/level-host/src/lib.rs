mod policy;

use level_cmd::{Changes, CommandError, Exec, OpInfo, Request, Response, Session, Success};
use level_render::Renderer;

pub use policy::Policy;

pub struct HostReply {
    pub response: Response,
    pub png: Option<Vec<u8>>,
}

impl HostReply {
    fn error(e: &CommandError) -> Self {
        Self {
            response: Response::error(e),
            png: None,
        }
    }
}

/// A session plus what clients need around it: permission gating and a long-lived renderer.
pub struct Host {
    session: Session,
    renderer: Option<Renderer>,
    policy: Policy,
}

impl Host {
    pub fn new(session: Session, policy: Policy) -> Self {
        Self {
            session,
            renderer: None,
            policy,
        }
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub fn policy(&self) -> Policy {
        self.policy
    }

    pub fn into_session(self) -> Session {
        self.session
    }

    pub fn execute(&mut self, request: Request, client: &str) -> HostReply {
        if let Err(e) = self.policy.check(&request) {
            return HostReply::error(&e);
        }
        if OpInfo::find(&request.op).is_some_and(|o| o.exec == Exec::Client) {
            return self.client_op(request);
        }
        HostReply {
            response: self.session.execute(request, client),
            png: None,
        }
    }

    pub fn execute_value(&mut self, request: serde_json::Value, client: &str) -> HostReply {
        match Request::from_value(request) {
            Ok(r) => self.execute(r, client),
            Err(e) => HostReply::error(&e),
        }
    }

    fn client_op(&mut self, request: Request) -> HostReply {
        match self.screenshot(request) {
            Ok(reply) => reply,
            Err(e) => HostReply::error(&e),
        }
    }

    fn screenshot(&mut self, request: Request) -> Result<HostReply, CommandError> {
        if let Some(expected) = request.expect_revision
            && expected != self.session.revision()
        {
            return Err(CommandError::RevisionMismatch {
                expected,
                actual: self.session.revision(),
            });
        }
        if self.renderer.is_none() {
            self.renderer = Some(Renderer::new()?);
        }
        let out_path = request
            .args
            .get("path")
            .and_then(serde_json::Value::as_str)
            .map(std::path::PathBuf::from);
        let renderer = self.renderer.as_ref().expect("created above");
        let shot = level_render::screenshot_with_selection(
            renderer,
            self.session.document(),
            self.session.selection(),
            request.args,
        )?;
        let mut result = shot.result();
        if let Some(path) = &out_path {
            let io = |e: &dyn std::fmt::Display| CommandError::Io {
                message: format!("{}: {e}", path.display()),
            };
            if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                std::fs::create_dir_all(dir).map_err(|e| io(&e))?;
            }
            level_fmt::write_atomic(path, &shot.png).map_err(|e| io(&e))?;
            result["path"] = serde_json::json!(path.display().to_string());
        }
        let response = Response::Success(Success {
            ok: true,
            revision: self.session.revision(),
            result,
            changes: Changes::default(),
            warnings: shot.warnings.clone(),
        });
        Ok(HostReply {
            response,
            png: Some(shot.png),
        })
    }
}
