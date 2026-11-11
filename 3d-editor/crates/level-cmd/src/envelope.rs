use level_core::ObjectId;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{CommandError, ErrorResponse};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub op: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub expect_revision: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge_key: Option<String>,
}

impl Request {
    pub fn new(op: impl Into<String>, args: Value) -> Self {
        Self {
            op: op.into(),
            args,
            dry_run: false,
            expect_revision: None,
            merge_key: None,
        }
    }

    pub fn from_value(value: Value) -> Result<Self, CommandError> {
        serde_json::from_value(value).map_err(|e| CommandError::InvalidArgument {
            field: None,
            message: format!("malformed request: {e}"),
            hint: Some("a request is {\"op\": \"...\", \"args\": {...}}".into()),
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Changes {
    pub created: Vec<ObjectId>,
    pub modified: Vec<ObjectId>,
    pub deleted: Vec<ObjectId>,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.created.is_empty() && self.modified.is_empty() && self.deleted.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Success<R = Value> {
    pub ok: bool,
    pub revision: u64,
    pub result: R,
    pub changes: Changes,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Response {
    Success(Success),
    Error(ErrorResponse),
}

impl Response {
    pub fn is_ok(&self) -> bool {
        matches!(self, Response::Success(_))
    }

    pub fn error(err: &CommandError) -> Self {
        Response::Error(err.to_response())
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).expect("responses serialize")
    }
}
