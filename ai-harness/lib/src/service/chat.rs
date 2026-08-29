//! Driving a conversation: `send_message`, `approve_tools`, `cancel_pending`.
//!
//! The rule this module exists to enforce — **a streamed turn is persisted
//! only at its terminal outcome, in one transaction, never mid-stream**:
//!
//! 1. The user's message is written first, in its own transaction, before
//!    the model is ever called — a conversation ending in a user message is
//!    always sendable, so a crash here just leaves something to retry.
//! 2. `Agent::stream_turn`/`resume_stream` is driven to completion, forwarding
//!    every [`AgentEvent`] to the caller's [`super::EventSink`]. Nothing is
//!    written to the database while this runs.
//! 3. On the terminal `AgentEvent::Turn`, `db::turns::settle` commits
//!    whatever the turn produced — messages, tool call state, and the
//!    conversation's timestamps — in one transaction.
//! 4. On a stream error, `db::turns::fail` records the failure and nothing
//!    else: a truncated assistant message is never replayable (it can end
//!    mid-`ToolUse`, or carry a `Thinking` block whose `signature` never
//!    arrived), so persisting it would leave a conversation that 400s on the
//!    next send with nothing in the schema marking it as poisoned.

use std::sync::Arc;

use futures_util::StreamExt;
use shared::agent::{AgentEvent, AgentTurn, ToolDecision, TurnStop};
use shared::conversation::{RunStatus, TurnOutcome};
use shared::error::ErrorReport;
use shared::ids::{ConversationId, TurnId};
use shared::llm::message::{ContentBlock, Message};

use crate::agent::{build_agent, AgentStream};
use crate::db::{self, DbError};

use super::{runs, EventSink, Service, ServiceError};

impl Service {
    /// Append `text` as a user turn and drive the agent forward, forwarding
    /// every event to `sink` and returning once the turn reaches a terminal
    /// outcome (or fails).
    pub async fn send_message(
        &self,
        conversation_id: ConversationId,
        text: String,
        sink: &dyn EventSink,
    ) -> Result<TurnOutcome, ServiceError> {
        let _guard = self.try_lock_conversation(conversation_id)?;
        // Holding the lock proves no live future owns a `running` row for
        // this conversation — any such row is an orphan from a crash or
        // from Tauri dropping a previous command's future.
        db::reap_orphaned(self.db.pool(), Some(conversation_id)).await?;

        let agent_config = db::conversations::agent_config(&self.db, conversation_id).await?;
        let ctx = self.tool_context(conversation_id).await?;
        let agent = Arc::new(build_agent(&agent_config, &self.tools, &self.router, ctx)?);

        let mut conversation = db::conversations::load_conversation(&self.db, conversation_id).await?;
        let user_message = Message::user(vec![ContentBlock::Text { text }]);
        conversation.messages.push(user_message.clone());

        let turn_id = db::turns::begin(&self.db, conversation_id, Some(&user_message)).await?;

        self.drive(conversation_id, turn_id, agent.stream_turn(conversation), sink)
            .await
    }

    /// Resolve a suspended turn's pending tool calls and drive the agent
    /// forward from there.
    pub async fn approve_tools(
        &self,
        conversation_id: ConversationId,
        decisions: Vec<ToolDecision>,
        sink: &dyn EventSink,
    ) -> Result<TurnOutcome, ServiceError> {
        let _guard = self.try_lock_conversation(conversation_id)?;
        db::reap_orphaned(self.db.pool(), Some(conversation_id)).await?;

        let agent_config = db::conversations::agent_config(&self.db, conversation_id).await?;
        let ctx = self.tool_context(conversation_id).await?;
        let agent = Arc::new(build_agent(&agent_config, &self.tools, &self.router, ctx)?);

        let (turn_id, turn) = reopen(&self.db, conversation_id, &decisions).await?;

        self.drive(conversation_id, turn_id, agent.resume_stream(turn, decisions), sink)
            .await
    }

    /// Discard a suspended turn without resolving it. The conversation is
    /// left exactly as it was before the turn started — see
    /// `db::turns::abandon`.
    pub async fn cancel_pending(&self, conversation_id: ConversationId) -> Result<(), ServiceError> {
        let _guard = self.try_lock_conversation(conversation_id)?;
        db::turns::abandon(&self.db, conversation_id)
            .await
            .map_err(|err| not_found_to_no_pending(conversation_id, err))
    }

    /// The detached counterpart to [`Service::send_message`]: reserves
    /// `conversation_id`'s run and drives the turn on a task that outlives
    /// this call, instead of a future scoped to one Tauri command
    /// invocation. Every [`AgentEvent`] still lands wherever
    /// [`Service::attach`] is currently following, exactly as it would over
    /// a direct `send_message`'s sink — attaching costs no accumulator of
    /// its own, since `Run` records the identical event stream.
    ///
    /// The reservation is synchronous: a second call while one is already
    /// registered fails immediately with `ServiceError::ConversationBusy`,
    /// before any task is ever spawned. Must be called from inside a Tokio
    /// runtime — every async Tauri command already runs on one.
    pub fn start_message(self: &Arc<Self>, conversation_id: ConversationId, text: String) -> Result<(), ServiceError> {
        let run = self.runs.begin(conversation_id)?;
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let status = match this.send_message(conversation_id, text, run.as_ref()).await {
                Ok(outcome) => RunStatus::Finished { outcome },
                Err(err) => RunStatus::Failed { error: (&err).into() },
            };
            run.finish(status);
            this.runs.end(conversation_id);
        });
        Ok(())
    }

    /// The detached counterpart to [`Service::approve_tools`] — see
    /// [`Service::start_message`]'s doc for what "detached" buys.
    pub fn start_approve_tools(
        self: &Arc<Self>,
        conversation_id: ConversationId,
        decisions: Vec<ToolDecision>,
    ) -> Result<(), ServiceError> {
        let run = self.runs.begin(conversation_id)?;
        let this = Arc::clone(self);
        tokio::spawn(async move {
            let status = match this.approve_tools(conversation_id, decisions, run.as_ref()).await {
                Ok(outcome) => RunStatus::Finished { outcome },
                Err(err) => RunStatus::Failed { error: (&err).into() },
            };
            run.finish(status);
            this.runs.end(conversation_id);
        });
        Ok(())
    }

    /// Replay everything `conversation_id`'s run has emitted so far into
    /// `emit`, then forward live events until the run settles or `emit`
    /// itself declines to continue.
    ///
    /// `Some(RunStatus::Idle)` if nothing is running for this conversation
    /// — the caller's own `get_conversation` is already current. `None`
    /// only when `emit` returned `false` before the run reached a terminal
    /// status: see [`runs::follow`]'s doc for why that case reports nothing
    /// rather than a status a dead caller can't use anyway.
    pub async fn attach(
        &self,
        conversation_id: ConversationId,
        emit: impl FnMut(&AgentEvent) -> bool,
    ) -> Option<RunStatus> {
        match self.runs.get(conversation_id) {
            Some(run) => runs::follow(run, emit).await,
            None => {
                tracing::trace!(
                    conversation_id = conversation_id.get(),
                    "attach found no run registered; reporting idle without subscribing"
                );
                Some(RunStatus::Idle)
            }
        }
    }

    async fn drive(
        &self,
        conversation_id: ConversationId,
        turn_id: TurnId,
        mut stream: AgentStream,
        sink: &dyn EventSink,
    ) -> Result<TurnOutcome, ServiceError> {
        let mut terminal: Option<AgentTurn> = None;
        while let Some(item) = stream.next().await {
            match item {
                Ok(event) => {
                    sink.emit(&event);
                    if let AgentEvent::Turn(turn) = event {
                        terminal = Some(turn);
                    }
                }
                Err(err) => {
                    db::turns::fail(&self.db, turn_id, &ErrorReport::from(&err)).await?;
                    return Err(ServiceError::Agent(err));
                }
            }
        }

        let Some(turn) = terminal else {
            // `Agent::run_stream` yields either a `Turn` or an `Err` on
            // every exit path — this is unreachable today, but reported as
            // a failure rather than silently treated as a done turn if that
            // ever stops being true.
            let report = ErrorReport {
                kind: shared::error::ErrorKind::Agent,
                provider: None,
                message: "the agent's event stream ended without a terminal event".to_string(),
                retryable: false,
            };
            db::turns::fail(&self.db, turn_id, &report).await?;
            return Err(ServiceError::TruncatedStream);
        };

        db::turns::settle(&self.db, turn_id, &turn).await?;
        outcome(&self.db, conversation_id, turn_id, turn).await
    }
}

/// `db::turns::reopen`'s `NotFound { entity: "pending_turn", .. }` becomes
/// `ServiceError::NoPendingTurn` — a caller-facing "there's nothing to
/// approve here," not a generic storage error.
async fn reopen(
    db: &db::Db,
    conversation_id: ConversationId,
    decisions: &[ToolDecision],
) -> Result<(TurnId, AgentTurn), ServiceError> {
    db::turns::reopen(db, conversation_id, decisions)
        .await
        .map_err(|err| not_found_to_no_pending(conversation_id, err))
}

fn not_found_to_no_pending(conversation_id: ConversationId, err: DbError) -> ServiceError {
    match err {
        DbError::NotFound {
            entity: "pending_turn",
            ..
        } => ServiceError::NoPendingTurn {
            conversation_id: conversation_id.get(),
        },
        other => ServiceError::Db(other),
    }
}

async fn outcome(
    db: &db::Db,
    conversation_id: ConversationId,
    turn_id: TurnId,
    turn: AgentTurn,
) -> Result<TurnOutcome, ServiceError> {
    Ok(match turn.stop {
        TurnStop::Done { stop_reason } => {
            let messages = messages_for_turn(db, conversation_id, turn_id).await?;
            TurnOutcome::Done {
                turn_id,
                steps: turn.steps,
                usage: turn.usage,
                stop_reason,
                messages,
            }
        }
        TurnStop::MaxSteps => {
            let messages = messages_for_turn(db, conversation_id, turn_id).await?;
            TurnOutcome::MaxSteps {
                turn_id,
                steps: turn.steps,
                usage: turn.usage,
                messages,
            }
        }
        TurnStop::AwaitingApproval { pending, completed } => TurnOutcome::AwaitingApproval {
            turn_id,
            pending,
            completed,
        },
    })
}

async fn messages_for_turn(
    db: &db::Db,
    conversation_id: ConversationId,
    turn_id: TurnId,
) -> Result<Vec<shared::conversation::StoredMessage>, ServiceError> {
    let all = db::conversations::load_messages(db, conversation_id).await?;
    Ok(all.into_iter().filter(|m| m.turn_id == Some(turn_id)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tool::{tool, JsonSchema, ToolOutput};
    use crate::db::Db;
    use crate::llm::router::{ProviderEntry, Router};
    use crate::llm::testing::ScriptedProvider;
    use serde::Deserialize;
    use shared::agent::{AgentConfigInput, Decision};
    use shared::llm::message::{CompletedMessage, Role, StopReason};
    use shared::llm::model::ModelRef;
    use shared::llm::tool::Thinking;
    use std::sync::Mutex as StdMutex;

    #[derive(Deserialize, JsonSchema)]
    struct DeployArgs {}

    struct CollectingSink(StdMutex<Vec<AgentEvent>>);

    impl CollectingSink {
        fn new() -> Self {
            Self(StdMutex::new(Vec::new()))
        }

        fn events(&self) -> Vec<AgentEvent> {
            self.0.lock().unwrap().clone()
        }
    }

    impl EventSink for CollectingSink {
        fn emit(&self, event: &AgentEvent) {
            self.0.lock().unwrap().push(event.clone());
        }
    }

    fn text_response(text: &str) -> Result<CompletedMessage, crate::llm::error::LlmError> {
        Ok(CompletedMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::Text { text: text.to_string() }],
            stop_reason: StopReason::EndTurn,
            usage: Default::default(),
            model: Some("test-model".to_string()),
        })
    }

    async fn service_with_script(
        responses: Vec<Result<CompletedMessage, crate::llm::error::LlmError>>,
        requires_approval: bool,
    ) -> Service {
        let db = Db::in_memory().await.unwrap();
        let scripted = ScriptedProvider::new(responses);
        let mut router = Router::new();
        router.register(
            "scripted",
            ProviderEntry {
                chat: Some(Arc::new(scripted)),
                ..Default::default()
            },
        );
        let mut registry = crate::agent::ToolRegistry::new();
        let mut deploy = tool("deploy", "Deploy to prod", |_args: DeployArgs| async move {
            Ok::<_, anyhow::Error>(ToolOutput::from("deployed"))
        });
        if requires_approval {
            deploy = deploy.requiring_approval();
        }
        registry.register(deploy);
        Service::new(db, Arc::new(router), Arc::new(registry))
    }

    async fn seed_conversation(service: &Service, tools: Vec<String>) -> ConversationId {
        let agent = service
            .create_agent(AgentConfigInput {
                name: "ops".to_string(),
                description: None,
                model: ModelRef::new("scripted", "test-model"),
                system: vec![],
                max_tokens: 256,
                tools,
                tool_choice: None,
                thinking: Thinking::default(),
                stop_sequences: vec![],
                max_steps: 8,
            })
            .await
            .unwrap();
        service
            .create_conversation(agent.id, shared::project::ProjectRef::Default, None)
            .await
            .unwrap()
            .id
    }

    #[tokio::test]
    async fn send_message_persists_the_added_messages_and_streams_every_event() {
        let service = service_with_script(vec![text_response("hi there")], false).await;
        let conv = seed_conversation(&service, vec![]).await;

        let sink = CollectingSink::new();
        let outcome = service.send_message(conv, "hello".to_string(), &sink).await.unwrap();

        assert!(matches!(outcome, TurnOutcome::Done { .. }));
        let events = sink.events();
        assert!(matches!(events.first(), Some(AgentEvent::StepStart { step: 0 })));
        assert!(matches!(events.last(), Some(AgentEvent::Turn(_))));

        let view = service.get_conversation(conv).await.unwrap();
        assert_eq!(view.messages.len(), 2, "user + assistant");
        assert!(view.pending.is_none());
    }

    #[tokio::test]
    async fn a_second_send_message_while_one_is_running_is_conversation_busy() {
        // A script with zero responses never resolves the first call within
        // this test's synchronous setup — instead, prove busy-ness the
        // deterministic way: hold the lock directly, exactly as a
        // concurrent in-flight `send_message` would.
        let service = service_with_script(vec![text_response("hi")], false).await;
        let conv = seed_conversation(&service, vec![]).await;
        let _guard = service.try_lock_conversation(conv).unwrap();

        let sink = CollectingSink::new();
        let err = service
            .send_message(conv, "hello".to_string(), &sink)
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::ConversationBusy { .. }));
    }

    #[tokio::test]
    async fn full_cycle_send_then_await_approval_then_approve_persists_once() {
        let service = service_with_script(
            vec![
                Ok(CompletedMessage {
                    role: Role::Assistant,
                    content: vec![ContentBlock::ToolUse {
                        id: "toolu_1".to_string(),
                        name: "deploy".to_string(),
                        input: serde_json::json!({}),
                    }],
                    stop_reason: StopReason::ToolUse,
                    usage: Default::default(),
                    model: Some("test-model".to_string()),
                }),
                text_response("Deployed."),
            ],
            true,
        )
        .await;
        let conv = seed_conversation(&service, vec!["deploy".to_string()]).await;

        let sink = CollectingSink::new();
        let outcome = service
            .send_message(conv, "deploy please".to_string(), &sink)
            .await
            .unwrap();
        let TurnOutcome::AwaitingApproval { turn_id, pending, .. } = outcome else {
            panic!("expected AwaitingApproval");
        };
        assert_eq!(pending.len(), 1);
        let tool_use_id = pending[0].tool_use_id.clone();

        let view = service.get_conversation(conv).await.unwrap();
        assert!(view.pending.is_some());
        assert_eq!(view.messages.len(), 1, "only the user message is persisted while suspended");
        assert!(
            view.decisions.is_empty(),
            "nothing has been decided yet — the user hasn't answered"
        );

        let sink2 = CollectingSink::new();
        let outcome2 = service
            .approve_tools(
                conv,
                vec![ToolDecision {
                    tool_use_id: tool_use_id.clone(),
                    decision: Decision::Approve,
                }],
                &sink2,
            )
            .await
            .unwrap();
        let TurnOutcome::Done { turn_id: final_turn_id, .. } = outcome2 else {
            panic!("expected Done");
        };
        assert_eq!(final_turn_id, turn_id);

        let view = service.get_conversation(conv).await.unwrap();
        assert!(view.pending.is_none());
        assert_eq!(view.messages.len(), 4, "user, tool_use, tool_result, final text");

        assert_eq!(view.decisions.len(), 1);
        assert_eq!(view.decisions[0].turn_id, turn_id);
        assert_eq!(view.decisions[0].tool_use_id, tool_use_id);
        assert_eq!(view.decisions[0].decision, shared::agent::Decision::Approve);
        assert_eq!(view.decisions[0].decided_by, Some(shared::agent::DecidedBy::User));
    }

    #[tokio::test]
    async fn approve_tools_on_a_conversation_with_nothing_pending_errors() {
        let service = service_with_script(vec![], false).await;
        let conv = seed_conversation(&service, vec![]).await;
        let sink = CollectingSink::new();
        let err = service
            .approve_tools(
                conv,
                vec![ToolDecision {
                    tool_use_id: "call_1_0".to_string(),
                    decision: Decision::Approve,
                }],
                &sink,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::NoPendingTurn { .. }));
    }

    #[tokio::test]
    async fn a_mid_stream_provider_error_leaves_only_the_user_message() {
        let db = Db::in_memory().await.unwrap();
        let scripted = ScriptedProvider::new(vec![Err(crate::llm::error::LlmError::Status {
            provider: "scripted",
            status: 500,
            kind: crate::llm::error::ApiErrorKind::Server,
            code: None,
            message: "boom".to_string(),
            request_id: None,
        })]);
        let mut router = Router::new();
        router.register(
            "scripted",
            ProviderEntry {
                chat: Some(Arc::new(scripted)),
                ..Default::default()
            },
        );
        let service = Service::new(db, Arc::new(router), Arc::new(crate::agent::ToolRegistry::new()));
        let conv = seed_conversation(&service, vec![]).await;

        let sink = CollectingSink::new();
        let err = service
            .send_message(conv, "hello".to_string(), &sink)
            .await
            .unwrap_err();
        assert!(matches!(err, ServiceError::Agent(_)));
        let report: ErrorReport = match &err {
            ServiceError::Agent(inner) => inner.into(),
            _ => unreachable!(),
        };
        assert!(report.retryable, "a 500 should be reported as retryable");

        let view = service.get_conversation(conv).await.unwrap();
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].role, Role::User);

        // The lock was released and the turn's `running` row was resolved
        // to `failed` (not left `running`), so the conversation is free to
        // try again — a fresh lock acquisition succeeds immediately.
        let _guard = service.try_lock_conversation(conv).unwrap();
    }

    #[tokio::test]
    async fn start_message_and_attach_sees_exactly_what_a_direct_send_message_produces() {
        let direct = service_with_script(vec![text_response("hi there")], false).await;
        let direct_conv = seed_conversation(&direct, vec![]).await;
        let sink = CollectingSink::new();
        direct.send_message(direct_conv, "hello".to_string(), &sink).await.unwrap();

        let detached = Arc::new(service_with_script(vec![text_response("hi there")], false).await);
        let conv = seed_conversation(&detached, vec![]).await;
        detached.start_message(conv, "hello".to_string()).unwrap();

        let mut attached = Vec::new();
        let status = detached
            .attach(conv, |event| {
                attached.push(event.clone());
                true
            })
            .await;

        assert!(matches!(status, Some(RunStatus::Finished { .. })));
        // `AgentEvent` derives no `PartialEq` — compare the two event
        // sequences structurally via their JSON encoding instead.
        assert_eq!(
            serde_json::to_value(&attached).unwrap(),
            serde_json::to_value(sink.events()).unwrap(),
        );
    }

    #[tokio::test]
    async fn a_second_start_message_while_one_is_registered_is_conversation_busy() {
        let service = Arc::new(service_with_script(vec![text_response("hi")], false).await);
        let conv = seed_conversation(&service, vec![]).await;
        service.start_message(conv, "hello".to_string()).unwrap();
        let err = service.start_message(conv, "again".to_string()).unwrap_err();
        assert!(matches!(err, ServiceError::ConversationBusy { .. }));
    }

    #[tokio::test]
    async fn attach_on_an_idle_conversation_reports_idle() {
        let service = service_with_script(vec![], false).await;
        let conv = seed_conversation(&service, vec![]).await;
        let status = service.attach(conv, |_event| true).await;
        assert_eq!(status, Some(RunStatus::Idle));
    }

    #[tokio::test]
    async fn start_message_with_a_mid_stream_error_reports_failed_and_frees_the_conversation() {
        let db = Db::in_memory().await.unwrap();
        let scripted = ScriptedProvider::new(vec![Err(crate::llm::error::LlmError::Status {
            provider: "scripted",
            status: 500,
            kind: crate::llm::error::ApiErrorKind::Server,
            code: None,
            message: "boom".to_string(),
            request_id: None,
        })]);
        let mut router = Router::new();
        router.register(
            "scripted",
            ProviderEntry {
                chat: Some(Arc::new(scripted)),
                ..Default::default()
            },
        );
        let service = Arc::new(Service::new(db, Arc::new(router), Arc::new(crate::agent::ToolRegistry::new())));
        let conv = seed_conversation(&service, vec![]).await;

        service.start_message(conv, "hello".to_string()).unwrap();
        let status = service.attach(conv, |_event| true).await;
        let Some(RunStatus::Failed { error }) = status else {
            panic!("expected Failed, got {status:?}");
        };
        assert!(error.retryable, "a 500 should be reported as retryable");

        let view = service.get_conversation(conv).await.unwrap();
        assert_eq!(view.messages.len(), 1);
        assert_eq!(view.messages[0].role, Role::User);

        // The run was deregistered on settle (a later attach sees nothing
        // running), and the fine-grained lock was released too.
        assert_eq!(service.attach(conv, |_event| true).await, Some(RunStatus::Idle));
        let _guard = service.try_lock_conversation(conv).unwrap();
    }

    #[tokio::test]
    async fn attach_declining_early_does_not_interrupt_the_turn_still_running() {
        let service = Arc::new(service_with_script(vec![text_response("hi there")], false).await);
        let conv = seed_conversation(&service, vec![]).await;
        service.start_message(conv, "hello".to_string()).unwrap();

        // Detach as soon as the first event arrives — the dead-webview case.
        let early = service.attach(conv, |_event| false).await;
        assert_eq!(early, None);

        // The turn is still running regardless: attaching again, without
        // declining this time, rides it to completion.
        let status = service.attach(conv, |_event| true).await;
        assert!(matches!(status, Some(RunStatus::Finished { .. })));

        let view = service.get_conversation(conv).await.unwrap();
        assert_eq!(view.messages.len(), 2, "user + assistant");
    }
}
