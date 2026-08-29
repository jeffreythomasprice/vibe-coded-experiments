//! `Project`: a named collection of directories that defines an agent's
//! sandbox.
//!
//! A project's virtual filesystem is a *filter* over the real one, not a
//! relocation: each configured directory keeps its real absolute path, and
//! nothing else on the machine exists inside it. See `lib::vfs::MountTable`
//! (built from a project's `dirs`) for the resolution rules, and
//! `lib::sandbox` for how the same directories become a `bwrap` argv for
//! subprocesses.
//!
//! The **default project** is not a database row. It is the built-in,
//! undeletable, zero-directory project every conversation starts under
//! unless another project is chosen — mirroring how `shared::theme::BuiltIn`
//! is a compiled-in enum, never a row in `themes`, and
//! `built_in_name_conflict` (not a SQL constraint) is the other half of that
//! module's name-uniqueness rule. Same split here: the `projects` table's
//! unique index covers user projects, [`default_project_name_conflict`]
//! covers the one reserved name it can't see.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ids::ProjectId;

/// The reserved name of the default project. No user project may take it —
/// see [`default_project_name_conflict`].
pub const DEFAULT_PROJECT_NAME: &str = "Default";

/// Whether a directory in a project is writable by the sandbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    #[default]
    ReadWrite,
    ReadOnly,
}

/// One directory a project exposes, and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectDir {
    pub path: PathBuf,
    #[serde(default)]
    pub mode: AccessMode,
}

/// A saved project definition, as CRUD returns it. Never represents the
/// default project — see the module doc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    pub id: ProjectId,
    #[serde(flatten)]
    pub input: ProjectInput,
    /// ISO8601 UTC, millisecond precision — same shape as `AgentConfig`'s.
    pub created_at: String,
    pub updated_at: String,
}

/// The editable fields of a [`Project`] — what `create`/`update` take.
/// Mirrors `shared::agent::AgentConfigInput`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectInput {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub dirs: Vec<ProjectDir>,
}

/// Which project a conversation is started under. `Default` never resolves to
/// a database row — see [`ProjectSnapshot`].
///
/// Adjacently tagged (`content = "id"`), not internally tagged: `ProjectId` is
/// a `#[serde(transparent)]` bare-number newtype, and serde's internal tagging
/// can't merge a tag object with a variant that serializes as a bare scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ProjectRef {
    Default,
    User(ProjectId),
}

/// A project resolved for execution: what `lib::vfs`/`lib::sandbox` build a
/// mount table from. Unlike `shared::agent::AgentConfig` (frozen verbatim into
/// `conversations.agent_config_json` at creation, since a system prompt is a
/// premise that must not change under a turn already run), a project is a
/// *live grant* — `lib::service::Service::conversation_mounts` re-resolves it
/// from `conversations.project_id` on every use, precisely so that removing a
/// directory from a project takes effect immediately for every conversation
/// open on it, not just new ones. `project_id: None` (never assigned, or the
/// project was since deleted) resolves to `dirs: vec![]` — an empty virtual
/// filesystem, the same safe fallback either way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectSnapshot {
    pub project_id: Option<ProjectId>,
    pub name: String,
    #[serde(default)]
    pub dirs: Vec<ProjectDir>,
}

impl ProjectSnapshot {
    /// The empty, zero-directory snapshot every conversation gets unless a
    /// user project is chosen.
    pub fn default_project() -> Self {
        Self {
            project_id: None,
            name: DEFAULT_PROJECT_NAME.to_string(),
            dirs: Vec::new(),
        }
    }
}

/// Whether `lib::sandbox`'s configured backend actually works on this
/// machine — the settings-page-visible proof that the sandboxing mechanism
/// is live, not just configured. See `lib::sandbox::detect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxStatus {
    pub backend: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Whether `name` collides with the default project's reserved name,
/// case-insensitively. The database's unique index on `projects.name` only
/// sees other user projects, so this is the other half of "a project name
/// must be unique across every project, default and user-created."
pub fn default_project_name_conflict(name: &str) -> bool {
    name.eq_ignore_ascii_case(DEFAULT_PROJECT_NAME)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> ProjectInput {
        ProjectInput {
            name: "webapp".to_string(),
            description: Some("The webapp checkout".to_string()),
            dirs: vec![
                ProjectDir {
                    path: PathBuf::from("/srv/webapp"),
                    mode: AccessMode::ReadOnly,
                },
                ProjectDir {
                    path: PathBuf::from("/home/jeff/scratch"),
                    mode: AccessMode::ReadWrite,
                },
            ],
        }
    }

    #[test]
    fn project_round_trips_through_json_with_id_and_timestamps_flattened_in() {
        let project = Project {
            id: ProjectId(1),
            input: input(),
            created_at: "2026-08-22T00:00:00.000Z".to_string(),
            updated_at: "2026-08-22T00:00:00.000Z".to_string(),
        };
        let json = serde_json::to_value(&project).unwrap();
        assert_eq!(json["id"], serde_json::json!(1));
        assert_eq!(json["name"], serde_json::json!("webapp"));
        let back: Project = serde_json::from_value(json).unwrap();
        assert_eq!(back, project);
    }

    #[test]
    fn project_dir_defaults_to_read_write_when_mode_is_omitted() {
        let dir: ProjectDir = serde_json::from_value(serde_json::json!({ "path": "/a/b" })).unwrap();
        assert_eq!(dir.mode, AccessMode::ReadWrite);
    }

    #[test]
    fn project_input_round_trips() {
        let original = input();
        let json = serde_json::to_string(&original).unwrap();
        let back: ProjectInput = serde_json::from_str(&json).unwrap();
        assert_eq!(back, original);
    }

    #[test]
    fn project_ref_tagged_encoding() {
        assert_eq!(
            serde_json::to_value(ProjectRef::Default).unwrap(),
            serde_json::json!({ "kind": "default" })
        );
        assert_eq!(
            serde_json::to_value(ProjectRef::User(ProjectId(5))).unwrap(),
            serde_json::json!({ "kind": "user", "id": 5 })
        );
        let back: ProjectRef = serde_json::from_value(serde_json::json!({ "kind": "user", "id": 5 })).unwrap();
        assert_eq!(back, ProjectRef::User(ProjectId(5)));
    }

    #[test]
    fn default_project_snapshot_is_empty() {
        let snapshot = ProjectSnapshot::default_project();
        assert_eq!(snapshot.project_id, None);
        assert!(snapshot.dirs.is_empty());
    }

    #[test]
    fn default_project_name_conflict_is_case_insensitive_and_none_for_a_novel_name() {
        assert!(default_project_name_conflict("Default"));
        assert!(default_project_name_conflict("DEFAULT"));
        assert!(default_project_name_conflict("default"));
        assert!(!default_project_name_conflict("webapp"));
    }
}
