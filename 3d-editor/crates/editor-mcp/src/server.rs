use std::sync::Arc;

use base64::Engine;
use editor_cmd::{CommandError, Request, Response};
use editor_proto::{Client, Policy, Reply};
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};
use serde_json::{Map, Value};

use crate::tools::{find_op, visible_tools};

const INSTRUCTIONS: &str = "\
A brush scene editor. Everything is addressed by stable ids (b brush, m mesh, e entity, g group, l layer, c camera).

The document lives in a editor-server that other clients (the GUI, the CLI, other agents) may share: a human may be watching your edits live. While you hold a transaction (history_begin) others cannot mutate, so keep transactions short; a SessionBusy error means someone else holds one. If expect_revision fails, someone else edited: re-read state. session_list shows the open sessions and session_attach switches this connection to another (by id or file path).

Workflow: file_info first; if its path is null, bind a file with file_save {path} (or file_open an existing scene) before editing. Then scene_summary to see what exists; open a transaction with history_begin (or use `batch`) \
for multi-step edits so they undo as one step; run check_validate; then view_screenshot from a couple of \
cameras (viewport.persp, viewport.top, viewport.front, viewport.side) with the `labels` overlay to map pixels \
to object ids; adjust; history_commit (or history_rollback). Errors are structured: read `code`, `hint` and \
`field`. Use dry_run on a mutation to preview it. Changes only reach disk with file_save.

Materials and textures: a face names a material (face_set_material); material_create defines it, with an image given as a file (store relative to the scene file, absolute, or embed) or as base64 bytes, plus color, emissive, roughness, metallic and flags. Save the scene before using relative paths (file_save). Align textures with face_align_uv (fit, justify, flip, reset; as_group continues one image across several faces), face_set_uv (offset in texels, scale in world units per texel, rotation in degrees; mode relative nudges), face_transfer_uv (copy or wrap an image onto neighbouring faces). Check the result numerically with face_get_uv (corner texels and uvs) and visually with view_screenshot (draws the images; textures false for flat colours). Transforms and vertex moves keep textures attached to the geometry unless texture_lock is false. select_faces_set/get share the face selection with the GUI.

In `batch`, items are {op, args, as?}; op names may be dotted or underscored. Reference earlier results as \"$name\" (the item's `as`), \"$N\", \"$name.ids[k]\" or \"$name/v2\" (a vertex or face of a created mesh).

Tools you cannot see are not enabled: mutating tools need the server started with --allow-mutate and \
destructive ones with --allow-destructive.";

pub struct EditorServer {
    client: Arc<Client>,
    tools: Arc<Vec<Tool>>,
}

impl EditorServer {
    pub fn new(client: Arc<Client>, policy: Policy) -> Self {
        Self {
            client,
            tools: Arc::new(visible_tools(policy)),
        }
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

fn to_result(reply: Reply) -> CallToolResult {
    let is_error = !reply.is_ok();
    let structured = serde_json::to_value(&reply.response).expect("responses serialize");
    let text = structured.to_string();
    let mut content = vec![ContentBlock::text(text)];
    if let Some(png) = reply.png {
        let data = base64::engine::general_purpose::STANDARD.encode(png);
        content.push(ContentBlock::image(data, "image/png"));
    }
    let mut result = if is_error {
        CallToolResult::error(content)
    } else {
        CallToolResult::success(content)
    };
    result.structured_content = Some(structured);
    result
}

fn error_result(e: &CommandError) -> CallToolResult {
    to_result(Reply {
        response: Response::error(e),
        png: None,
    })
}

impl ServerHandler for EditorServer {
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

        let client = self.client.clone();
        let reply = tokio::task::spawn_blocking(move || client.request(envelope))
            .await
            .map_err(|e| McpError::internal_error(format!("command task failed: {e}"), None))?;
        match reply {
            Ok(reply) => Ok(to_result(reply).into()),
            Err(e) => Ok(error_result(&CommandError::from(e)).into()),
        }
    }
}
