use editor_cmd::{Changes, CommandError, Request, Response, Session, Success};
use editor_render::Renderer;

use crate::HostReply;

pub(crate) fn screenshot(
    renderer: &mut Option<Renderer>,
    session: &Session,
    request: Request,
) -> Result<HostReply, CommandError> {
    if let Some(expected) = request.expect_revision
        && expected != session.revision()
    {
        return Err(CommandError::RevisionMismatch {
            expected,
            actual: session.revision(),
        });
    }
    if renderer.is_none() {
        *renderer = Some(Renderer::new()?);
    }
    let out_path = request
        .args
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(std::path::PathBuf::from);
    let renderer = renderer.as_ref().expect("created above");
    let shot = editor_render::screenshot_with_selection(
        renderer,
        session.document(),
        session.selection(),
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
        editor_fmt::write_atomic(path, &shot.png).map_err(|e| io(&e))?;
        result.path = Some(path.display().to_string());
    }
    let response = Response::Success(Success {
        ok: true,
        revision: session.revision(),
        result: serde_json::to_value(result).expect("results serialize"),
        changes: Changes::default(),
        warnings: shot.warnings.clone(),
    });
    Ok(HostReply {
        response,
        png: Some(shot.png),
    })
}
