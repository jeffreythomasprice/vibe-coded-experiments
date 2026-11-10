use level_core::{GroupId, ObjectId};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LoadError {
    #[error("parse error: {message}")]
    Parse { message: String },
    #[error("file version {found} is not supported (latest is {supported})")]
    UnsupportedVersion { found: i64, supported: u32 },
    #[error("duplicate id {id}")]
    DuplicateId { id: String },
    #[error("{id}.{field} refers to missing {target}")]
    DanglingReference {
        id: ObjectId,
        field: &'static str,
        target: ObjectId,
    },
    #[error("group {id} is its own ancestor")]
    GroupCycle { id: GroupId },
    #[error("{id} has invalid geometry: {reason}")]
    InvalidGeometry { id: ObjectId, reason: String },
}
