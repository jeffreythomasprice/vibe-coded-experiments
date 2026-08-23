//! The service layer: what a Tauri command calls into. Wires
//! `lib::db` (persistence) to `lib::agent` (execution) — CRUD on agent
//! configs, and starting/driving/approving/deleting conversations.

pub mod agents;
pub mod chat;
pub mod conversations;
pub mod error;
pub mod preferences;
pub mod themes;

pub use error::ServiceError;

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::OwnedMutexGuard;

use shared::agent::AgentEvent;
use shared::ids::ConversationId;

use crate::agent::ToolRegistry;
use crate::config::Config;
use crate::db::Db;
use crate::llm::router::Router;

/// Where a turn's `AgentEvent`s go while it streams. `server` implements
/// this over a `tauri::ipc::Channel<AgentEvent>`; tests implement it over a
/// `Vec`. Keeping the driver — [`chat::Service::send_message`]/
/// `approve_tools`, and their transaction boundaries — independent of any
/// particular sink is what makes it unit-testable without Tauri, the same
/// split `lib::catalog` already makes for the command it backs.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &AgentEvent);
}

/// The no-op sink — for a caller that only wants the final [`shared::conversation::TurnOutcome`].
impl EventSink for () {
    fn emit(&self, _event: &AgentEvent) {}
}

pub struct Service {
    db: Db,
    router: Arc<Router>,
    tools: Arc<ToolRegistry>,
    /// One lock per conversation, held across the whole
    /// begin → stream → settle span in [`chat`] that no single database
    /// transaction can cover. Swept of entries nobody holds on every
    /// acquisition, so the map stays proportional to conversations
    /// currently streaming, not to all of them. `turns_one_open_per_conversation`
    /// (see `sql/0001_init.sql`) is the durable backstop behind the same
    /// guarantee, in case this lock is ever bypassed.
    locks: std::sync::Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
}

impl Service {
    pub fn new(db: Db, router: Arc<Router>, tools: Arc<ToolRegistry>) -> Self {
        Self {
            db,
            router,
            tools,
            locks: std::sync::Mutex::new(HashMap::new()),
        }
    }

    /// Open the configured database, build a `Router` from the configured
    /// providers, and start with an empty tool registry — callers that ship
    /// built-in tools register them before handing this to a Tauri command.
    pub async fn from_config(config: &Config) -> anyhow::Result<Self> {
        let db = Db::open(&config.database).await?;
        let router = Arc::new(Router::from_config(&config.llm));
        let tools = Arc::new(ToolRegistry::new());
        Ok(Self::new(db, router, tools))
    }

    /// `try_lock_owned`, not a blocking lock: a second turn on a
    /// conversation that is already streaming fails immediately with
    /// [`ServiceError::ConversationBusy`] rather than queuing behind one
    /// that may take minutes — the UI disables its send control while a
    /// turn is running; this exists to catch a double-click or a stale
    /// client, not to serve as a work queue.
    fn try_lock_conversation(&self, id: ConversationId) -> Result<OwnedMutexGuard<()>, ServiceError> {
        let mutex = {
            let mut locks = self.locks.lock().expect("conversation lock map poisoned");
            locks.retain(|_, m| Arc::strong_count(m) > 1);
            locks
                .entry(id.get())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        mutex
            .try_lock_owned()
            .map_err(|_| ServiceError::ConversationBusy { conversation_id: id.get() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn empty_service() -> Service {
        let db = Db::in_memory().await.unwrap();
        Service::new(db, Arc::new(Router::new()), Arc::new(ToolRegistry::new()))
    }

    #[tokio::test]
    async fn locking_the_same_conversation_twice_is_conversation_busy() {
        let service = empty_service().await;
        let id = ConversationId(1);
        let _guard = service.try_lock_conversation(id).unwrap();
        let err = service.try_lock_conversation(id).unwrap_err();
        assert!(matches!(err, ServiceError::ConversationBusy { conversation_id } if conversation_id == 1));
    }

    #[tokio::test]
    async fn locking_different_conversations_never_conflicts() {
        let service = empty_service().await;
        let _a = service.try_lock_conversation(ConversationId(1)).unwrap();
        let _b = service.try_lock_conversation(ConversationId(2)).unwrap();
    }

    #[tokio::test]
    async fn dropping_the_guard_frees_the_conversation_to_lock_again() {
        let service = empty_service().await;
        let id = ConversationId(1);
        {
            let _guard = service.try_lock_conversation(id).unwrap();
        }
        let _guard = service.try_lock_conversation(id).unwrap();
    }
}
