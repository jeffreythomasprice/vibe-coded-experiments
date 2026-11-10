use std::sync::{Arc, Mutex};

use base64::Engine;
use level_cmd::{CommandError, Request, Response};
use level_host::{Host, HostReply};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{Map, Value};

use crate::tools::{find_op, visible_tools};

const INSTRUCTIONS: &str = "\
A brush level editor. Everything is addressed by stable ids (b brush, m mesh, e entity, g group, l layer, c camera).

Workflow: file_info first; if its path is null, bind a file with file_save {path} (or file_open an existing level) before editing. Then scene_summary to see what exists; open a transaction with history_begin (or use `batch`) \
for multi-step edits so they undo as one step; run check_validate; then view_screenshot from a couple of \
cameras (viewport.persp, viewport.top, viewport.front, viewport.side) with the `labels` overlay to map pixels \
to object ids; adjust; history_commit (or history_rollback). Errors are structured: read `code`, `hint` and \
`field`. Use dry_run on a mutation to preview it. Changes only reach disk with file_save.

Tools you cannot see are not enabled: mutating tools need the server started with --allow-mutate and \
destructive ones with --allow-destructive.";

pub struct LevelServer {
    host: Arc<Mutex<Host>>,
    tools: Arc<Vec<Tool>>,
}

impl LevelServer {
    pub fn new(host: Host) -> Self {
        let tools = Arc::new(visible_tools(host.policy()));
        Self {
            host: Arc::new(Mutex::new(host)),
            tools,
        }
    }

    pub fn host(&self) -> Arc<Mutex<Host>> {
        self.host.clone()
    }
}

fn split_envelope(
    mut args: Map<String, Value>,
) -> Result<(Value, bool, Option<u64>), CommandError> {
    let dry_run = match args.remove("dry_run") {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => b,
        Some(_) => {
            return Err(CommandError::invalid_arg(
                "dry_run",
                "dry_run must be a boolean",
            ));
        }
    };
    let expect_revision = match args.remove("expect_revision") {
        None | Some(Value::Null) => None,
        Some(v) => Some(v.as_u64().ok_or_else(|| {
            CommandError::invalid_arg(
                "expect_revision",
                "expect_revision must be a non-negative integer",
            )
        })?),
    };
    Ok((Value::Object(args), dry_run, expect_revision))
}

fn to_result(reply: HostReply) -> CallToolResult {
    let is_error = !reply.response.is_ok();
    let text = serde_json::to_string(&reply.response.to_value()).expect("responses serialize");
    let mut content = vec![ContentBlock::text(text)];
    if let Some(png) = reply.png {
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        content.push(ContentBlock::image(data, "image/png"));
    }
    if is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    }
}

fn error_result(e: &CommandError) -> CallToolResult {
    to_result(HostReply {
        response: Response::error(e),
        png: None,
    })
}

impl ServerHandler for LevelServer {
    #[allow(deprecated)]
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.instructions = Some(INSTRUCTIONS.to_string());
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult::with_all_items((*self.tools).clone()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let Some(info) = find_op(&request.name) else {
            return Err(McpError::invalid_params(
                format!("unknown tool `{}`", request.name),
                None,
            ));
        };
        let (args, dry_run, expect_revision) =
            match split_envelope(request.arguments.unwrap_or_default()) {
                Ok(parts) => parts,
                Err(e) => return Ok(error_result(&e).into()),
            };
        let mut envelope = Request::new(info.name, args);
        envelope.dry_run = dry_run;
        envelope.expect_revision = expect_revision;

        let host = self.host.clone();
        let reply = tokio::task::spawn_blocking(move || {
            let mut host = host.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            host.execute(envelope, "mcp")
        })
        .await
        .map_err(|e| McpError::internal_error(format!("command task failed: {e}"), None))?;
        Ok(to_result(reply).into())
    }
}
