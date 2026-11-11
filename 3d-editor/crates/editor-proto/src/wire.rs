use editor_cmd::{Request, Response};
use editor_core::{ElementRef, Object, ObjectId};
use schemars::{JsonSchema, Schema, schema_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use editor_cmd::results::{ClientRow, SessionRow};

pub const PROTOCOL: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ControlAction {
    Status,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum ClientMsg {
    Hello {
        id: u64,
        protocol: u32,
        client: String,
        mutate: bool,
        destructive: bool,
        #[serde(default)]
        cwd: Option<String>,
    },
    Request {
        id: u64,
        request: Request,
    },
    Control {
        id: u64,
        action: ControlAction,
        #[serde(default)]
        force: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HelloResult {
    pub conn: u64,
    pub protocol: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ControlStatus {
    pub protocol: u32,
    pub connections: usize,
    pub dirty_sessions: usize,
    pub sessions: Vec<SessionRow>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct StopAck {
    pub stopping: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotRunning {
    pub running: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FileInfo {
    pub path: Option<String>,
    pub name: String,
    pub dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TxnWire {
    pub holder: String,
    pub conn: u64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HistoryInfo {
    pub can_undo: bool,
    pub can_redo: bool,
    pub transaction: Option<TxnWire>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SnapshotEvent {
    pub session: String,
    pub revision: u64,
    pub document_toml: String,
    pub selection: Vec<ObjectId>,
    #[serde(default)]
    pub face_selection: Vec<ElementRef>,
    pub file: FileInfo,
    pub history: HistoryInfo,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangedEvent {
    pub session: String,
    pub revision: u64,
    pub upsert: Vec<Object>,
    pub deleted: Vec<ObjectId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection: Option<Vec<ObjectId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face_selection: Option<Vec<ElementRef>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<FileInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history: Option<HistoryInfo>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Reply {
        id: u64,
        response: Response,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        png_base64: Option<String>,
    },
    Snapshot(SnapshotEvent),
    Changed(ChangedEvent),
}

/// JSON Schemas for everything that crosses the socket.
pub fn wire_schema() -> Value {
    let schema = |s: Schema| serde_json::to_value(s).expect("schemas serialize");
    json!({
        "protocol": PROTOCOL,
        "client_msg": schema(schema_for!(ClientMsg)),
        "server_msg": schema(schema_for!(ServerMsg)),
        "hello_result": schema(schema_for!(HelloResult)),
        "control_status": schema(schema_for!(ControlStatus)),
        "stop_ack": schema(schema_for!(StopAck)),
        "not_running": schema(schema_for!(NotRunning)),
    })
}
