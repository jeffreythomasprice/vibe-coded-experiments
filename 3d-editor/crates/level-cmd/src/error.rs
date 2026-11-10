use serde::Serialize;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CommandError {
    #[error("{target} not found")]
    NotFound { target: String },
    #[error("{target} is not a {expected}")]
    WrongKind { target: String, expected: String },
    #[error("{message}")]
    InvalidArgument {
        field: Option<String>,
        message: String,
        hint: Option<String>,
    },
    #[error("{message}")]
    InvalidGeometry {
        target: Option<String>,
        message: String,
        hint: Option<String>,
    },
    #[error("{target} is locked")]
    Locked { target: String },
    #[error("expected revision {expected} but the document is at {actual}")]
    RevisionMismatch { expected: u64, actual: u64 },
    #[error("no transaction is open")]
    NoTransaction,
    #[error("a transaction is already open")]
    TransactionOpen,
    #[error("the document has unsaved changes")]
    UnsavedChanges,
    #[error("{path} already exists")]
    FileExists { path: String },
    #[error("`{op}` needs {needs} access, which this server was not started with")]
    NotPermitted { op: String, needs: String },
    #[error("{message}")]
    Io { message: String },
    #[error("{message}")]
    Format { message: String },
    #[error("render failed: {message}")]
    Render { message: String },
    #[error("command {index}: {source}")]
    InBatch {
        index: usize,
        source: Box<CommandError>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ErrorResponse {
    pub ok: bool,
    pub code: &'static str,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    pub hint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
}

impl CommandError {
    pub fn not_found(target: impl ToString) -> Self {
        Self::NotFound {
            target: target.to_string(),
        }
    }

    pub fn wrong_kind(target: impl ToString, expected: &str) -> Self {
        Self::WrongKind {
            target: target.to_string(),
            expected: expected.to_string(),
        }
    }

    pub fn invalid_arg(field: &str, message: impl Into<String>) -> Self {
        Self::InvalidArgument {
            field: Some(field.to_string()),
            message: message.into(),
            hint: None,
        }
    }

    pub fn invalid_geometry(target: impl ToString, message: impl Into<String>, hint: &str) -> Self {
        Self::InvalidGeometry {
            target: Some(target.to_string()),
            message: message.into(),
            hint: Some(hint.to_string()),
        }
    }

    pub fn with_hint(mut self, new_hint: impl Into<String>) -> Self {
        match &mut self {
            Self::InvalidArgument { hint, .. } | Self::InvalidGeometry { hint, .. } => {
                *hint = Some(new_hint.into());
            }
            _ => {}
        }
        self
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::NotFound { .. } => "NotFound",
            Self::WrongKind { .. } => "WrongKind",
            Self::InvalidArgument { .. } => "InvalidArgument",
            Self::InvalidGeometry { .. } => "InvalidGeometry",
            Self::Locked { .. } => "Locked",
            Self::RevisionMismatch { .. } => "RevisionMismatch",
            Self::NoTransaction => "NoTransaction",
            Self::TransactionOpen => "TransactionOpen",
            Self::UnsavedChanges => "UnsavedChanges",
            Self::FileExists { .. } => "FileExists",
            Self::NotPermitted { .. } => "NotPermitted",
            Self::Io { .. } => "Io",
            Self::Format { .. } => "Format",
            Self::Render { .. } => "Render",
            Self::InBatch { source, .. } => source.code(),
        }
    }

    pub fn to_response(&self) -> ErrorResponse {
        if let Self::InBatch { index, source } = self {
            let mut r = source.to_response();
            r.index = Some(*index);
            r.message = format!("command {index}: {}", r.message);
            return r;
        }
        let (target, field, hint) = match self {
            Self::NotFound { target } => (
                Some(target.clone()),
                None,
                Some("use scene.summary or scene.get to find valid ids".to_string()),
            ),
            Self::WrongKind { target, expected } => (
                Some(target.clone()),
                None,
                Some(format!("pass the id of a {expected}")),
            ),
            Self::InvalidArgument { field, hint, .. } => (None, field.clone(), hint.clone()),
            Self::InvalidGeometry { target, hint, .. } => (target.clone(), None, hint.clone()),
            Self::Locked { target } => (Some(target.clone()), None, None),
            Self::RevisionMismatch { .. } => (
                None,
                Some("expect_revision".to_string()),
                Some("re-read the document state and retry with the current revision".to_string()),
            ),
            Self::NoTransaction => (None, None, Some("call history.begin first".to_string())),
            Self::TransactionOpen => (
                None,
                None,
                Some("call history.commit or history.rollback first".to_string()),
            ),
            Self::UnsavedChanges => (
                None,
                None,
                Some("save first with file.save, or pass discard: true".to_string()),
            ),
            Self::FileExists { .. } => (
                None,
                Some("path".to_string()),
                Some("pass overwrite: true to replace it, or use file.open to edit it".to_string()),
            ),
            Self::NotPermitted { needs, .. } => (
                None,
                None,
                Some(format!("restart the server with --allow-{needs}")),
            ),
            Self::Io { .. } | Self::Format { .. } | Self::Render { .. } | Self::InBatch { .. } => {
                (None, None, None)
            }
        };
        ErrorResponse {
            ok: false,
            code: self.code(),
            message: self.to_string(),
            target,
            field,
            hint: hint.unwrap_or_default(),
            index: None,
        }
    }
}
