use std::sync::Mutex;

use async_trait::async_trait;
use futures_util::StreamExt;
use serde::Deserialize;

use super::tool::{tool, JsonSchema, Tool, ToolOutput};
use super::*;
use crate::llm::error::LlmError;
use crate::llm::testing::ScriptedProvider;
use shared::llm::{CompletedMessage, Role, StopReason as SR, ToolDef};

fn model_ref() -> ModelRef {
    ModelRef::new("scripted", "test-model")
}

fn text_message(text: &str) -> CompletedMessage {
    CompletedMessage {
        role: Role::Assistant,
        content: vec![ContentBlock::Text {
            text: text.to_string(),
        }],
        stop_reason: SR::EndTurn,
        usage: Usage {
            input_tokens: 10,
            output_tokens: 5,
            ..Default::default()
        },
        model: Some("test-model".to_string()),
    }
}

fn tool_call_message(raw_id: &str, name: &str, input: serde_json::Value) -> CompletedMessage {
    CompletedMessage {
        role: Role::Assistant,
        content: vec![ContentBlock::ToolUse {
            id: raw_id.to_string(),
            name: name.to_string(),
            input,
        }],
        stop_reason: SR::ToolUse,
        usage: Usage {
            input_tokens: 20,
            output_tokens: 8,
            ..Default::default()
        },
        model: Some("test-model".to_string()),
    }
}

fn two_tool_call_message(
    id_a: &str,
    name_a: &str,
    input_a: serde_json::Value,
    id_b: &str,
    name_b: &str,
    input_b: serde_json::Value,
) -> CompletedMessage {
    CompletedMessage {
        role: Role::Assistant,
        content: vec![
            ContentBlock::ToolUse {
                id: id_a.to_string(),
                name: name_a.to_string(),
                input: input_a,
            },
            ContentBlock::ToolUse {
                id: id_b.to_string(),
                name: name_b.to_string(),
                input: input_b,
            },
        ],
        stop_reason: SR::ToolUse,
        usage: Usage::default(),
        model: Some("test-model".to_string()),
    }
}

fn unmodeled_tool_call_message() -> CompletedMessage {
    CompletedMessage {
        role: Role::Assistant,
        // A provider-native tool call this crate doesn't model (e.g.
        // Anthropic's server_tool_use) deserializes to `Unknown`, exactly
        // like this — see `shared::llm::message::ContentBlock`'s doc.
        content: vec![ContentBlock::Unknown],
        stop_reason: SR::ToolUse,
        usage: Usage::default(),
        model: Some("test-model".to_string()),
    }
}

/// A `Tool` that counts its own calls, for asserting a gated tool did *not*
/// run and an automatic one did.
struct CountingTool {
    def: ToolDef,
    approval: Approval,
    calls: Arc<Mutex<u32>>,
    result: String,
}

#[async_trait]
impl Tool for CountingTool {
    fn def(&self) -> &ToolDef {
        &self.def
    }

    fn approval(&self) -> Approval {
        self.approval
    }

    async fn call(&self, _input: serde_json::Value) -> anyhow::Result<ToolOutput> {
        *self.calls.lock().unwrap() += 1;
        Ok(ToolOutput::text(self.result.clone()))
    }
}

fn counting_tool(name: &str, approval: Approval, result: &str) -> (CountingTool, Arc<Mutex<u32>>) {
    let calls = Arc::new(Mutex::new(0));
    let tool = CountingTool {
        def: ToolDef {
            name: name.to_string(),
            description: "a test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        },
        approval,
        calls: calls.clone(),
        result: result.to_string(),
    };
    (tool, calls)
}

fn first_pending_id(turn: &AgentTurn) -> String {
    match &turn.stop {
        TurnStop::AwaitingApproval { pending, .. } => pending[0].tool_use_id.clone(),
        other => panic!("expected AwaitingApproval, got {other:?}"),
    }
}

/// Runs a turn that stops on one gated `deploy` call, so approval tests
/// don't each re-derive the setup.
async fn awaiting_turn_with_one_gated_call() -> (Agent, AgentTurn, Arc<Mutex<u32>>) {
    let (deploy, calls) = counting_tool("deploy", Approval::RequiresApproval, "deployed");
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "raw",
            "deploy",
            serde_json::json!({"env": "prod"}),
        )),
        Ok(text_message("done")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(deploy)
        .build_with(provider)
        .unwrap();
    let turn = agent.next_turn(Conversation::default()).await.unwrap();
    (agent, turn, calls)
}

fn tool_result_ids(conversation: &Conversation) -> Vec<&str> {
    conversation
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------
// The basic loop
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_turn_with_no_tool_calls_is_done_after_one_step() {
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(text_message("hi there"))]));
    let agent = Agent::builder(model_ref(), 1024)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    assert_eq!(turn.steps, 1);
    assert_eq!(turn.added.len(), 1);
    assert!(matches!(
        turn.stop,
        TurnStop::Done {
            stop_reason: SR::EndTurn
        }
    ));
    assert_eq!(turn.usage.input_tokens, 10);
}

#[tokio::test]
async fn an_automatic_tool_runs_and_the_follow_up_ends_the_turn() {
    let (weather, calls) = counting_tool("get_weather", Approval::Automatic, "72F and sunny");
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "raw",
            "get_weather",
            serde_json::json!({"city": "Paris"}),
        )),
        Ok(text_message("It's 72F and sunny in Paris.")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(weather)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    assert_eq!(turn.steps, 2);
    // assistant(tool_use) + user(tool_result) + assistant(final text)
    assert_eq!(turn.added.len(), 3);
    assert!(matches!(
        turn.stop,
        TurnStop::Done {
            stop_reason: SR::EndTurn
        }
    ));
    assert_eq!(turn.usage.input_tokens, 30);
    assert_eq!(turn.usage.output_tokens, 13);
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn a_tool_returning_err_becomes_an_is_error_result_and_the_loop_continues() {
    struct FailingTool(ToolDef);
    #[async_trait]
    impl Tool for FailingTool {
        fn def(&self) -> &ToolDef {
            &self.0
        }
        async fn call(&self, _input: serde_json::Value) -> anyhow::Result<ToolOutput> {
            Err(anyhow::anyhow!("connection refused"))
        }
    }
    let failing = FailingTool(ToolDef {
        name: "flaky".to_string(),
        description: "always fails".to_string(),
        input_schema: serde_json::json!({"type": "object"}),
    });
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message("raw", "flaky", serde_json::json!({}))),
        Ok(text_message("sorry about that")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(failing)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    assert!(matches!(turn.stop, TurnStop::Done { .. }));
    let result = turn
        .conversation
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                is_error, content, ..
            } => Some((*is_error, content.clone())),
            _ => None,
        })
        .unwrap();
    assert!(result.0);
    match &result.1[0] {
        ToolResultContent::Text { text } => assert!(text.contains("connection refused")),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[tokio::test]
async fn an_unregistered_tool_name_becomes_an_is_error_result_naming_it() {
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "raw",
            "mystery_tool",
            serde_json::json!({}),
        )),
        Ok(text_message("ok")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    let result = turn
        .conversation
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                is_error, content, ..
            } => Some((*is_error, content.clone())),
            _ => None,
        })
        .unwrap();
    assert!(result.0);
    match &result.1[0] {
        ToolResultContent::Text { text } => assert!(text.contains("mystery_tool")),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
struct WeatherArgs {
    city: String,
}

#[tokio::test]
async fn input_json_that_does_not_match_the_tool_schema_becomes_an_is_error_result() {
    let weather = tool(
        "get_weather",
        "look up weather",
        |args: WeatherArgs| async move { Ok(format!("sunny in {}", args.city)) },
    );
    // The model sends a field name the schema doesn't have.
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "raw",
            "get_weather",
            serde_json::json!({"not_city": "Paris"}),
        )),
        Ok(text_message("ok")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(weather)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    let result = turn
        .conversation
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult { is_error, .. } => Some(*is_error),
            _ => None,
        })
        .unwrap();
    assert!(
        result,
        "a schema-mismatched tool input must become an is_error result, not abort the turn"
    );
}

#[tokio::test]
async fn max_steps_is_enforced_and_the_loop_does_not_hang() {
    let (weather, _) = counting_tool("get_weather", Approval::Automatic, "sunny");
    // Every response asks for another tool call — the model never stops on
    // its own, so only `max_steps` keeps this test finite.
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "raw0",
            "get_weather",
            serde_json::json!({}),
        )),
        Ok(tool_call_message(
            "raw1",
            "get_weather",
            serde_json::json!({}),
        )),
        Ok(tool_call_message(
            "raw2",
            "get_weather",
            serde_json::json!({}),
        )),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(weather)
        .max_steps(2)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    assert_eq!(turn.steps, 2);
    assert!(matches!(turn.stop, TurnStop::MaxSteps));
}

#[tokio::test]
async fn stop_reason_tool_use_with_no_tool_use_blocks_is_an_error_not_a_false_done() {
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(
        unmodeled_tool_call_message(),
    )]));
    let agent = Agent::builder(model_ref(), 1024)
        .build_with(provider)
        .unwrap();

    let err = agent.next_turn(Conversation::default()).await.unwrap_err();

    assert!(matches!(
        err,
        AgentError::UnhandledToolUse {
            stop_reason: SR::ToolUse
        }
    ));
}

// ---------------------------------------------------------------------
// Approval gating
// ---------------------------------------------------------------------

#[tokio::test]
async fn a_tool_needing_approval_suspends_the_turn_and_does_not_run() {
    let (_agent, turn, calls) = awaiting_turn_with_one_gated_call().await;

    match &turn.stop {
        TurnStop::AwaitingApproval { pending, completed } => {
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].name, "deploy");
            assert!(completed.is_empty());
        }
        other => panic!("expected AwaitingApproval, got {other:?}"),
    }
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn mixed_automatic_and_gated_only_the_gated_one_is_pending() {
    let (weather, weather_calls) = counting_tool("get_weather", Approval::Automatic, "sunny");
    let (deploy, deploy_calls) = counting_tool("deploy", Approval::RequiresApproval, "deployed");
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(two_tool_call_message(
        "raw0",
        "get_weather",
        serde_json::json!({}),
        "raw1",
        "deploy",
        serde_json::json!({}),
    ))]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(weather)
        .tool(deploy)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    match &turn.stop {
        TurnStop::AwaitingApproval { pending, completed } => {
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].name, "deploy");
            assert_eq!(completed.len(), 1);
        }
        other => panic!("expected AwaitingApproval, got {other:?}"),
    }
    assert_eq!(
        *weather_calls.lock().unwrap(),
        1,
        "the automatic call must have run already"
    );
    assert_eq!(
        *deploy_calls.lock().unwrap(),
        0,
        "the gated call must not have run yet"
    );
}

#[tokio::test]
async fn resume_approve_runs_the_tool_and_every_tool_use_gets_a_result() {
    let (agent, turn, calls) = awaiting_turn_with_one_gated_call().await;
    let id = first_pending_id(&turn);

    let resumed = agent
        .resume(
            turn,
            &[ToolDecision {
                tool_use_id: id.clone(),
                decision: Decision::Approve,
            }],
        )
        .await
        .unwrap();

    assert!(matches!(resumed.stop, TurnStop::Done { .. }));
    assert_eq!(*calls.lock().unwrap(), 1);
    let is_error = resumed
        .conversation
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                is_error,
                ..
            } if *tool_use_id == id => Some(*is_error),
            _ => None,
        });
    assert_eq!(is_error, Some(false));
}

#[tokio::test]
async fn resume_deny_inserts_a_placeholder_and_never_runs_the_tool() {
    let (agent, turn, calls) = awaiting_turn_with_one_gated_call().await;
    let id = first_pending_id(&turn);

    let resumed = agent
        .resume(
            turn,
            &[ToolDecision {
                tool_use_id: id.clone(),
                decision: Decision::Deny {
                    reason: Some("no budget this quarter".to_string()),
                },
            }],
        )
        .await
        .unwrap();

    assert!(
        matches!(resumed.stop, TurnStop::Done { .. }),
        "the model still gets a follow-up turn"
    );
    assert_eq!(*calls.lock().unwrap(), 0);
    let (is_error, content) = resumed
        .conversation
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                is_error,
                content,
            } if *tool_use_id == id => Some((*is_error, content.clone())),
            _ => None,
        })
        .unwrap();
    assert!(is_error);
    match &content[0] {
        ToolResultContent::Text { text } => assert_eq!(text, "no budget this quarter"),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[tokio::test]
async fn two_calls_to_the_same_tool_name_in_one_step_merge_back_in_original_order() {
    let (deploy, calls) = counting_tool("deploy", Approval::RequiresApproval, "deployed");
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(two_tool_call_message(
            "raw0",
            "deploy",
            serde_json::json!({"env": "staging"}),
            "raw1",
            "deploy",
            serde_json::json!({"env": "prod"}),
        )),
        Ok(text_message("done")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(deploy)
        .build_with(provider)
        .unwrap();
    let turn = agent.next_turn(Conversation::default()).await.unwrap();
    let pending = match &turn.stop {
        TurnStop::AwaitingApproval { pending, .. } => pending.clone(),
        other => panic!("expected AwaitingApproval, got {other:?}"),
    };
    assert_eq!(pending.len(), 2);
    let first_id = pending[0].tool_use_id.clone();
    let second_id = pending[1].tool_use_id.clone();

    let resumed = agent
        .resume(
            turn,
            &[
                ToolDecision {
                    tool_use_id: first_id.clone(),
                    decision: Decision::Deny {
                        reason: Some("not now".to_string()),
                    },
                },
                ToolDecision {
                    tool_use_id: second_id.clone(),
                    decision: Decision::Approve,
                },
            ],
        )
        .await
        .unwrap();

    // The tool-result message resolving both calls is the one right before
    // the model's follow-up text.
    let resolving = &resumed.conversation.messages[resumed.conversation.messages.len() - 2];
    let ids: Vec<&str> = resolving
        .content
        .iter()
        .filter_map(|b| match b {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ids,
        vec![first_id.as_str(), second_id.as_str()],
        "results must come back in the same order the model asked for the calls, \
         not the order decisions were given in"
    );
    assert_eq!(*calls.lock().unwrap(), 1, "only the approved call may run");
}

#[tokio::test]
async fn resume_on_a_turn_that_is_not_awaiting_approval_errors() {
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(text_message("hi"))]));
    let agent = Agent::builder(model_ref(), 1024)
        .build_with(provider)
        .unwrap();
    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    let err = agent.resume(turn, &[]).await.unwrap_err();

    assert!(matches!(err, AgentError::NotAwaitingApproval));
}

#[tokio::test]
async fn resume_with_a_missing_decision_errors() {
    let (agent, turn, _calls) = awaiting_turn_with_one_gated_call().await;
    let err = agent.resume(turn, &[]).await.unwrap_err();
    assert!(matches!(err, AgentError::MissingDecision { .. }));
}

#[tokio::test]
async fn resume_with_an_unknown_decision_errors() {
    let (agent, turn, _calls) = awaiting_turn_with_one_gated_call().await;
    let id = first_pending_id(&turn);
    let decisions = vec![
        ToolDecision {
            tool_use_id: id,
            decision: Decision::Approve,
        },
        ToolDecision {
            tool_use_id: "not-actually-pending".to_string(),
            decision: Decision::Approve,
        },
    ];

    let err = agent.resume(turn, &decisions).await.unwrap_err();

    assert!(
        matches!(err, AgentError::UnknownDecision { tool_use_id } if tool_use_id == "not-actually-pending")
    );
}

#[tokio::test]
async fn resume_after_the_conversation_was_mutated_is_rejected() {
    let (agent, mut turn, _calls) = awaiting_turn_with_one_gated_call().await;
    let decisions = vec![ToolDecision {
        tool_use_id: first_pending_id(&turn),
        decision: Decision::Approve,
    }];
    turn.conversation
        .messages
        .push(Message::user(vec![ContentBlock::Text {
            text: "sneaky".to_string(),
        }]));

    let err = agent.resume(turn, &decisions).await.unwrap_err();

    assert!(matches!(err, AgentError::ConversationMismatch));
}

// ---------------------------------------------------------------------
// Turn-unique tool ids
// ---------------------------------------------------------------------

#[tokio::test]
async fn provider_supplied_ids_that_collide_across_steps_are_rewritten_to_be_turn_unique() {
    let (weather, _) = counting_tool("get_weather", Approval::Automatic, "sunny");
    // Both steps' raw ids collide, mimicking Ollama's per-response counter
    // restart (`ollama::wire::synth_tool_id`).
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "ollama-tool-0",
            "get_weather",
            serde_json::json!({"city": "Paris"}),
        )),
        Ok(tool_call_message(
            "ollama-tool-0",
            "get_weather",
            serde_json::json!({"city": "Rome"}),
        )),
        Ok(text_message("done")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(weather)
        .build_with(provider)
        .unwrap();

    let turn = agent.next_turn(Conversation::default()).await.unwrap();

    assert!(matches!(turn.stop, TurnStop::Done { .. }));
    let tool_use_ids: Vec<&str> = turn
        .conversation
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(tool_use_ids.len(), 2);
    assert_ne!(
        tool_use_ids[0], tool_use_ids[1],
        "ids from two different steps must not collide after rewriting"
    );
    assert_eq!(
        tool_result_ids(&turn.conversation),
        tool_use_ids,
        "each tool_result must reference its own step's rewritten id"
    );
}

// ---------------------------------------------------------------------
// send / options passed to the provider
// ---------------------------------------------------------------------

#[tokio::test]
async fn send_appends_user_text_then_runs_the_turn() {
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(text_message("hello!"))]));
    let agent = Agent::builder(model_ref(), 1024)
        .build_with(provider.clone())
        .unwrap();

    let turn = agent
        .send(Conversation::default(), "hi there")
        .await
        .unwrap();

    assert_eq!(turn.steps, 1);
    let calls = provider.calls.lock().unwrap();
    let (conversation, _options) = &calls[0];
    assert_eq!(conversation.messages.len(), 1);
    match &conversation.messages[0].content[0] {
        ContentBlock::Text { text } => assert_eq!(text, "hi there"),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[tokio::test]
async fn the_provider_receives_the_joined_system_prompt_and_every_tool_def() {
    let (weather, _) = counting_tool("get_weather", Approval::Automatic, "sunny");
    let provider = Arc::new(ScriptedProvider::new(vec![Ok(text_message("ok"))]));
    let agent = Agent::builder(model_ref(), 1024)
        .system("You are a helpful assistant.")
        .system("Be concise.")
        .tool(weather)
        .build_with(provider.clone())
        .unwrap();

    agent.next_turn(Conversation::default()).await.unwrap();

    let calls = provider.calls.lock().unwrap();
    let (conversation, options) = &calls[0];
    assert_eq!(
        conversation.system.as_deref(),
        Some("You are a helpful assistant.\n\nBe concise.")
    );
    assert_eq!(options.tools.len(), 1);
    assert_eq!(options.tools[0].name, "get_weather");
}

#[tokio::test]
async fn a_forced_tool_choice_is_relaxed_to_auto_after_the_first_step() {
    let (weather, _) = counting_tool("get_weather", Approval::Automatic, "sunny");
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "raw",
            "get_weather",
            serde_json::json!({}),
        )),
        Ok(text_message("done")),
    ]));
    let agent = Agent::builder(model_ref(), 1024)
        .tool(weather)
        .tool_choice(ToolChoice::Any)
        .build_with(provider.clone())
        .unwrap();

    agent.next_turn(Conversation::default()).await.unwrap();

    let calls = provider.calls.lock().unwrap();
    assert_eq!(calls.len(), 2);
    assert!(matches!(calls[0].1.tool_choice, Some(ToolChoice::Any)));
    assert!(matches!(calls[1].1.tool_choice, Some(ToolChoice::Auto)));
}

// ---------------------------------------------------------------------
// Streaming
// ---------------------------------------------------------------------

fn multi_step_script() -> Vec<Result<CompletedMessage, LlmError>> {
    vec![
        Ok(tool_call_message(
            "raw",
            "get_weather",
            serde_json::json!({"city": "Paris"}),
        )),
        Ok(text_message("It's sunny in Paris.")),
    ]
}

#[tokio::test]
async fn stream_turn_accumulates_to_the_same_agent_turn_as_next_turn() {
    let (weather_a, _) = counting_tool("get_weather", Approval::Automatic, "72F sunny");
    let (weather_b, _) = counting_tool("get_weather", Approval::Automatic, "72F sunny");

    let blocking_provider = Arc::new(ScriptedProvider::new(multi_step_script()));
    let blocking_agent = Agent::builder(model_ref(), 1024)
        .tool(weather_a)
        .build_with(blocking_provider)
        .unwrap();
    let blocking_turn = blocking_agent
        .next_turn(Conversation::default())
        .await
        .unwrap();

    let streaming_provider = Arc::new(ScriptedProvider::new(multi_step_script()));
    let streaming_agent = Arc::new(
        Agent::builder(model_ref(), 1024)
            .tool(weather_b)
            .build_with(streaming_provider)
            .unwrap(),
    );
    let mut stream = streaming_agent.stream_turn(Conversation::default());
    let mut final_turn = None;
    while let Some(event) = stream.next().await {
        if let AgentEvent::Turn(turn) = event.unwrap() {
            final_turn = Some(turn);
        }
    }
    let streaming_turn = final_turn.expect("stream ended without a Turn event");

    assert_eq!(streaming_turn.steps, blocking_turn.steps);
    assert_eq!(streaming_turn.usage, blocking_turn.usage);
    assert_eq!(
        serde_json::to_value(&streaming_turn.conversation).unwrap(),
        serde_json::to_value(&blocking_turn.conversation).unwrap(),
        "streaming and blocking must produce byte-for-byte the same resulting conversation"
    );
}

#[tokio::test]
async fn the_stream_emits_tool_start_and_tool_end_around_a_tool_call_and_terminates_at_turn() {
    let (weather, calls) = counting_tool("get_weather", Approval::Automatic, "72F");
    let provider = Arc::new(ScriptedProvider::new(vec![
        Ok(tool_call_message(
            "raw",
            "get_weather",
            serde_json::json!({}),
        )),
        Ok(text_message("done")),
    ]));
    let agent = Arc::new(
        Agent::builder(model_ref(), 1024)
            .tool(weather)
            .build_with(provider)
            .unwrap(),
    );

    let mut stream = agent.stream_turn(Conversation::default());
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolStart { name, .. } if name == "get_weather")));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolEnd { name, .. } if name == "get_weather")));
    assert!(matches!(events.last(), Some(AgentEvent::Turn(_))));
    assert_eq!(*calls.lock().unwrap(), 1);
}

#[tokio::test]
async fn resume_stream_emits_tool_events_and_terminates_with_the_turn_resume_would_return() {
    let (agent, turn, calls) = awaiting_turn_with_one_gated_call().await;
    let agent = Arc::new(agent);
    let id = first_pending_id(&turn);
    let decisions = vec![ToolDecision {
        tool_use_id: id.clone(),
        decision: Decision::Approve,
    }];

    let mut stream = agent.clone().resume_stream(turn, decisions);
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }

    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolStart { tool_use_id, .. } if *tool_use_id == id)));
    assert!(events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolEnd { tool_use_id, .. } if *tool_use_id == id)));
    let final_turn = match events.pop() {
        Some(AgentEvent::Turn(turn)) => turn,
        other => panic!("expected the stream to end with Turn, got {other:?}"),
    };
    assert!(matches!(final_turn.stop, TurnStop::Done { .. }));
    assert_eq!(*calls.lock().unwrap(), 1);
}
