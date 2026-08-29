//! CRUD on projects, and the two ways a project turns into a
//! [`crate::vfs::MountTable`] — both live, never a frozen snapshot, and both
//! fail closed to the default project's empty virtual filesystem:
//!
//! - [`Service::resolve_project`] / [`Service::resolve_project_id`] — a
//!   [`ProjectRef`] chosen when *starting* a conversation.
//! - [`Service::conversation_mounts`] — an *existing* conversation's current
//!   `project_id`, re-read on every call. See `lib::db::projects`'s module
//!   doc and `sql/0004_projects.sql` for why this is never a stored
//!   snapshot the way `conversations.agent_config_json` is one.
//!
//! Also owns the validations the database can't express — mirroring
//! [`super::themes::validate_theme_input`]: a name may not collide with the
//! default project's reserved name, and every directory must be an
//! absolute, existing directory with no duplicates within the same project.

use std::collections::HashSet;

use shared::ids::{ConversationId, ProjectId};
use shared::project::{default_project_name_conflict, Project, ProjectInput, ProjectRef, ProjectSnapshot, SandboxStatus};

use crate::db;
use crate::sandbox::Availability;
use crate::vfs::MountTable;

use super::{Service, ServiceError};

impl Service {
    pub async fn list_projects(&self) -> Result<Vec<Project>, ServiceError> {
        Ok(db::projects::list(&self.db).await?)
    }

    pub async fn get_project(&self, id: ProjectId) -> Result<Project, ServiceError> {
        Ok(db::projects::get(&self.db, id).await?)
    }

    pub async fn create_project(&self, input: ProjectInput) -> Result<Project, ServiceError> {
        validate_project_input(&input)?;
        Ok(db::projects::create(&self.db, &input).await?)
    }

    pub async fn update_project(&self, id: ProjectId, input: ProjectInput) -> Result<Project, ServiceError> {
        validate_project_input(&input)?;
        Ok(db::projects::update(&self.db, id, &input).await?)
    }

    pub async fn delete_project(&self, id: ProjectId) -> Result<(), ServiceError> {
        Ok(db::projects::delete(&self.db, id).await?)
    }

    /// Every project in "start a new conversation" order: the default
    /// project first (never a row — see `shared::project`'s module doc),
    /// then user projects by name.
    pub async fn list_projects_for_picker(&self) -> Result<Vec<ProjectSnapshot>, ServiceError> {
        let mut snapshots = vec![ProjectSnapshot::default_project()];
        snapshots.extend(db::projects::list(&self.db).await?.into_iter().map(project_snapshot));
        Ok(snapshots)
    }

    /// Resolve a [`ProjectRef`] chosen when starting a conversation into its
    /// current directories. A [`ProjectRef::User`] naming a since-deleted id
    /// falls back to the default project — fail closed, same as
    /// [`Service::conversation_mounts`].
    pub async fn resolve_project(&self, project: ProjectRef) -> Result<ProjectSnapshot, ServiceError> {
        match project {
            ProjectRef::Default => Ok(ProjectSnapshot::default_project()),
            ProjectRef::User(id) => Ok(db::projects::find(&self.db, id)
                .await?
                .map(project_snapshot)
                .unwrap_or_else(ProjectSnapshot::default_project)),
        }
    }

    /// The `project_id` to store on a new conversation — `None` for the
    /// default project, matching `conversations.project_id`'s NULL
    /// convention. Unlike [`Service::resolve_project`], a
    /// [`ProjectRef::User`] naming a missing id is an error here: this is
    /// the moment a caller picked a specific project, so a stale or
    /// garbage id should fail loudly rather than silently downgrade to the
    /// default.
    pub async fn resolve_project_id(&self, project: ProjectRef) -> Result<Option<ProjectId>, ServiceError> {
        match project {
            ProjectRef::Default => Ok(None),
            ProjectRef::User(id) => {
                db::projects::get(&self.db, id).await?;
                Ok(Some(id))
            }
        }
    }

    /// The mount table an existing conversation may touch **right now** —
    /// resolved live from `conversations.project_id`, never frozen. Empty
    /// (no filesystem access at all) when the conversation has no project
    /// or its project was since deleted.
    pub async fn conversation_mounts(&self, id: ConversationId) -> Result<MountTable, ServiceError> {
        let dirs = db::projects::for_conversation(&self.db, id)
            .await?
            .map(|project| project.input.dirs)
            .unwrap_or_default();
        Ok(MountTable::build(&dirs)?)
    }

    /// Whether the configured sandbox backend actually works on this
    /// machine — the user-visible proof the mechanism is live. See
    /// `lib::sandbox::detect`, called once in [`Service::from_config`].
    pub fn sandbox_status(&self) -> SandboxStatus {
        let (available, reason) = match &self.sandbox_availability {
            Availability::Available => (true, None),
            Availability::Unavailable { reason } => (false, Some(reason.clone())),
        };
        SandboxStatus {
            backend: self.sandbox.name().to_string(),
            available,
            reason,
        }
    }
}

fn project_snapshot(project: Project) -> ProjectSnapshot {
    ProjectSnapshot {
        project_id: Some(project.id),
        name: project.input.name,
        dirs: project.input.dirs,
    }
}

/// The rules the database can't express: non-empty name, no collision with
/// the default project's reserved name, and every directory absolute,
/// existing, and not repeated within the same project.
fn validate_project_input(input: &ProjectInput) -> Result<(), ServiceError> {
    if input.name.trim().is_empty() {
        return Err(ServiceError::EmptyProjectName);
    }
    if default_project_name_conflict(&input.name) {
        return Err(ServiceError::ProjectNameConflict { name: input.name.clone() });
    }
    let mut seen = HashSet::new();
    for dir in &input.dirs {
        let shown = dir.path.display().to_string();
        if !dir.path.is_absolute() {
            return Err(ServiceError::InvalidProjectDir {
                path: shown,
                reason: "must be an absolute path".to_string(),
            });
        }
        if !dir.path.is_dir() {
            return Err(ServiceError::InvalidProjectDir {
                path: shown,
                reason: "does not exist or is not a directory".to_string(),
            });
        }
        if !seen.insert(dir.path.clone()) {
            return Err(ServiceError::InvalidProjectDir {
                path: shown,
                reason: "listed more than once in this project".to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::llm::router::Router;
    use shared::project::{AccessMode, ProjectDir};
    use std::path::PathBuf;
    use std::sync::Arc;

    async fn empty_service() -> Service {
        let db = Db::in_memory().await.unwrap();
        Service::new(db, Arc::new(Router::new()), Arc::new(crate::agent::ToolRegistry::new()))
    }

    /// A real, existing directory every test can point a project at without
    /// depending on anything outside the checkout.
    fn a_real_dir() -> PathBuf {
        std::env::temp_dir()
    }

    fn input(name: &str) -> ProjectInput {
        ProjectInput {
            name: name.to_string(),
            description: None,
            dirs: vec![ProjectDir {
                path: a_real_dir(),
                mode: AccessMode::ReadWrite,
            }],
        }
    }

    #[tokio::test]
    async fn create_then_list_round_trips() {
        let service = empty_service().await;
        let created = service.create_project(input("webapp")).await.unwrap();
        assert_eq!(service.list_projects().await.unwrap(), vec![created]);
    }

    #[tokio::test]
    async fn create_rejects_the_default_projects_reserved_name() {
        let service = empty_service().await;
        let err = service.create_project(input("Default")).await.unwrap_err();
        assert!(matches!(err, ServiceError::ProjectNameConflict { name } if name == "Default"));
    }

    #[tokio::test]
    async fn create_rejects_a_blank_name() {
        let service = empty_service().await;
        let err = service.create_project(input("   ")).await.unwrap_err();
        assert!(matches!(err, ServiceError::EmptyProjectName));
    }

    #[tokio::test]
    async fn create_rejects_a_relative_directory() {
        let service = empty_service().await;
        let mut bad = input("webapp");
        bad.dirs[0].path = PathBuf::from("relative/path");
        let err = service.create_project(bad).await.unwrap_err();
        assert!(matches!(err, ServiceError::InvalidProjectDir { .. }));
    }

    #[tokio::test]
    async fn create_rejects_a_missing_directory() {
        let service = empty_service().await;
        let mut bad = input("webapp");
        bad.dirs[0].path = PathBuf::from("/definitely/does/not/exist/anywhere");
        let err = service.create_project(bad).await.unwrap_err();
        assert!(matches!(err, ServiceError::InvalidProjectDir { .. }));
    }

    #[tokio::test]
    async fn create_rejects_a_duplicate_directory_within_one_project() {
        let service = empty_service().await;
        let mut dup = input("webapp");
        dup.dirs.push(dup.dirs[0].clone());
        let err = service.create_project(dup).await.unwrap_err();
        assert!(matches!(err, ServiceError::InvalidProjectDir { .. }));
    }

    #[tokio::test]
    async fn list_projects_for_picker_puts_the_default_first() {
        let service = empty_service().await;
        service.create_project(input("Apple")).await.unwrap();
        let picker = service.list_projects_for_picker().await.unwrap();
        assert_eq!(picker[0].project_id, None);
        assert_eq!(picker[0].name, "Default");
        assert_eq!(picker[1].name, "Apple");
    }

    #[tokio::test]
    async fn resolve_project_default_is_the_empty_snapshot() {
        let service = empty_service().await;
        let snapshot = service.resolve_project(ProjectRef::Default).await.unwrap();
        assert_eq!(snapshot.project_id, None);
        assert!(snapshot.dirs.is_empty());
    }

    #[tokio::test]
    async fn resolve_project_user_falls_back_to_default_for_a_missing_id() {
        let service = empty_service().await;
        let snapshot = service.resolve_project(ProjectRef::User(ProjectId(999))).await.unwrap();
        assert_eq!(snapshot.project_id, None);
        assert!(snapshot.dirs.is_empty());
    }

    #[tokio::test]
    async fn resolve_project_id_errors_on_a_missing_id_rather_than_silently_defaulting() {
        let service = empty_service().await;
        let err = service.resolve_project_id(ProjectRef::User(ProjectId(999))).await.unwrap_err();
        assert!(matches!(err, ServiceError::Db(crate::db::DbError::NotFound { .. })));
    }

    #[tokio::test]
    async fn conversation_mounts_is_empty_for_a_conversation_with_no_project() {
        let service = empty_service().await;
        let agent = service
            .create_agent(shared::agent::AgentConfigInput {
                name: "ops".to_string(),
                description: None,
                model: shared::llm::model::ModelRef::new("scripted", "test-model"),
                system: vec![],
                max_tokens: 256,
                tools: vec![],
                tool_choice: None,
                thinking: shared::llm::tool::Thinking::default(),
                stop_sequences: vec![],
                max_steps: 4,
            })
            .await
            .unwrap();
        let conversation = service
            .create_conversation(agent.id, ProjectRef::Default, None)
            .await
            .unwrap();
        let mounts = service.conversation_mounts(conversation.id).await.unwrap();
        assert!(mounts.is_empty());
    }

    #[tokio::test]
    async fn sandbox_status_reports_unavailable_when_constructed_via_new() {
        // `Service::new` never probes for a real backend (see its doc) —
        // real detection is covered by `lib::sandbox`'s own tests, which
        // exercise the actual `bwrap` probe end to end.
        let service = empty_service().await;
        let status = service.sandbox_status();
        assert!(!status.available);
        assert!(status.reason.is_some());
    }
}
