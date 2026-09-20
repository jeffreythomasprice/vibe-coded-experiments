use std::sync::Arc;

use crate::llm::LlmError;
use crate::llm::message::{Content, Message};
use crate::llm::provider::{ChatOptions, ChatRequest, Provider};
use crate::llm::tool::ToolRegistry;

const DEFAULT_MAX_TURNS: usize = 8;

pub struct Agent {
    provider: Arc<dyn Provider>,
    model: String,
    tools: ToolRegistry,
    options: ChatOptions,
    max_turns: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentOutcome {
    pub messages: Vec<Message>,
    pub text: String,
    pub turns: usize,
}

impl Agent {
    pub fn new(provider: Arc<dyn Provider>, model: impl Into<String>) -> Self {
        Self {
            provider,
            model: model.into(),
            tools: ToolRegistry::new(),
            options: ChatOptions::default(),
            max_turns: DEFAULT_MAX_TURNS,
        }
    }

    pub fn with_tools(mut self, tools: ToolRegistry) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_options(mut self, options: ChatOptions) -> Self {
        self.options = options;
        self
    }

    pub fn with_max_turns(mut self, max_turns: usize) -> Self {
        self.max_turns = max_turns;
        self
    }

    /// Runs the chat/tool loop until the model responds with no tool calls, or
    /// `max_turns` is exceeded. A tool that is unknown, rejects its arguments, or
    /// fails is not fatal: the error text becomes that call's tool-result message
    /// so the model can see it and try again. `max_turns` is the backstop against a
    /// tool that keeps failing forever.
    pub async fn run(&self, messages: Vec<Message>) -> Result<AgentOutcome, LlmError> {
        let mut transcript = messages;
        let tool_defs = self.tools.definitions();

        for turn in 1..=self.max_turns {
            let request = ChatRequest {
                model: self.model.clone(),
                messages: transcript.clone(),
                tools: tool_defs.clone(),
                options: self.options.clone(),
            };
            let response = self.provider.chat(&request).await?;
            let assistant = response.message;
            let tool_calls = assistant.tool_calls.clone();
            let text = assistant.text();
            transcript.push(assistant);

            if tool_calls.is_empty() {
                return Ok(AgentOutcome {
                    messages: transcript,
                    text,
                    turns: turn,
                });
            }

            tracing::debug!(
                target: "diffusion::llm",
                turn,
                tool_calls = tool_calls.len(),
                "agent turn requested tool calls"
            );

            for call in &tool_calls {
                let content = match self.tools.invoke(call).await {
                    Ok(output) => output.content,
                    Err(err) => {
                        tracing::warn!(
                            target: "diffusion::llm",
                            tool = %call.name,
                            error = %err,
                            "tool call failed"
                        );
                        vec![Content::text(err.to_string())]
                    }
                };
                transcript.push(Message::tool_result(call, content));
            }
        }

        Err(LlmError::MaxTurns {
            limit: self.max_turns,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::mock::ScriptedProvider;
    use crate::llm::provider::{ChatResponse, StopReason, Usage};
    use crate::llm::tool::{ToolCall, ToolFailure, ToolOutput};
    use crate::llm::{FunctionTool, Role};
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Deserialize, JsonSchema)]
    struct Echo {
        value: String,
    }

    fn echo_tool() -> FunctionTool {
        FunctionTool::sync::<Echo, _>("echo", "echoes its input", |args| {
            Ok(ToolOutput::text(format!("echo: {}", args.value)))
        })
    }

    fn final_reply(text: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(text),
            stop_reason: StopReason::Stop,
            usage: Usage::default(),
            logprobs: Vec::new(),
        }
    }

    fn tool_call_reply(calls: Vec<(&str, serde_json::Value)>) -> ChatResponse {
        let tool_calls: Vec<ToolCall> = calls
            .into_iter()
            .enumerate()
            .map(|(i, (name, arguments))| ToolCall {
                id: format!("call_{i}"),
                name: name.to_owned(),
                arguments,
            })
            .collect();
        ChatResponse {
            message: Message::assistant_with_tool_calls("", tool_calls),
            stop_reason: StopReason::ToolCalls,
            usage: Usage::default(),
            logprobs: Vec::new(),
        }
    }

    #[tokio::test]
    async fn no_tool_calls_is_a_single_turn() {
        let provider = Arc::new(ScriptedProvider::new(vec![final_reply("hi there")]));
        let agent = Agent::new(provider, "test-model");

        let outcome = agent.run(vec![Message::user("hello")]).await.unwrap();

        assert_eq!(outcome.turns, 1);
        assert_eq!(outcome.text, "hi there");
        assert_eq!(outcome.messages.len(), 2);
        assert_eq!(outcome.messages[0].role, Role::User);
        assert_eq!(outcome.messages[1].role, Role::Assistant);
    }

    #[tokio::test]
    async fn one_tool_call_feeds_result_back_into_next_request() {
        let provider = Arc::new(ScriptedProvider::new(vec![
            tool_call_reply(vec![("echo", json!({"value": "hi"}))]),
            final_reply("done"),
        ]));
        let mut tools = ToolRegistry::new();
        tools.register(echo_tool()).unwrap();

        let agent = Agent::new(provider.clone(), "test-model").with_tools(tools);
        let outcome = agent.run(vec![Message::user("say hi")]).await.unwrap();

        assert_eq!(outcome.turns, 2);
        let requests = provider.requests();
        assert_eq!(requests.len(), 2);
        let second_request_tool_message = requests[1]
            .messages
            .iter()
            .find(|m| m.role == Role::Tool)
            .expect("second request should include the tool result");
        assert_eq!(second_request_tool_message.text(), "echo: hi");
        assert_eq!(
            second_request_tool_message.tool_name.as_deref(),
            Some("echo")
        );
    }

    #[tokio::test]
    async fn several_tool_calls_in_one_turn_all_run_in_order() {
        let provider = Arc::new(ScriptedProvider::new(vec![
            tool_call_reply(vec![
                ("echo", json!({"value": "first"})),
                ("echo", json!({"value": "second"})),
            ]),
            final_reply("done"),
        ]));
        let mut tools = ToolRegistry::new();
        tools.register(echo_tool()).unwrap();

        let agent = Agent::new(provider, "test-model").with_tools(tools);
        let outcome = agent.run(vec![Message::user("go")]).await.unwrap();

        let tool_messages: Vec<_> = outcome
            .messages
            .iter()
            .filter(|m| m.role == Role::Tool)
            .map(Message::text)
            .collect();
        assert_eq!(tool_messages, vec!["echo: first", "echo: second"]);
    }

    #[tokio::test]
    async fn unknown_tool_call_becomes_an_error_message_and_the_loop_continues() {
        let provider = Arc::new(ScriptedProvider::new(vec![
            tool_call_reply(vec![("nonexistent", json!({}))]),
            final_reply("recovered"),
        ]));
        let agent = Agent::new(provider, "test-model");

        let outcome = agent.run(vec![Message::user("go")]).await.unwrap();

        assert_eq!(outcome.text, "recovered");
        let tool_message = outcome
            .messages
            .iter()
            .find(|m| m.role == Role::Tool)
            .unwrap();
        assert!(tool_message.text().contains("nonexistent"));
    }

    #[tokio::test]
    async fn failing_tool_becomes_an_error_message_and_the_loop_continues() {
        let mut tools = ToolRegistry::new();
        tools
            .register(FunctionTool::sync::<Echo, _>("failer", "always fails", |_| {
                Err(ToolFailure::new("boom"))
            }))
            .unwrap();
        let provider = Arc::new(ScriptedProvider::new(vec![
            tool_call_reply(vec![("failer", json!({"value": "x"}))]),
            final_reply("recovered"),
        ]));

        let agent = Agent::new(provider, "test-model").with_tools(tools);
        let outcome = agent.run(vec![Message::user("go")]).await.unwrap();

        assert_eq!(outcome.text, "recovered");
        let tool_message = outcome
            .messages
            .iter()
            .find(|m| m.role == Role::Tool)
            .unwrap();
        assert!(tool_message.text().contains("boom"));
    }

    #[tokio::test]
    async fn exceeding_max_turns_is_an_error() {
        let provider = Arc::new(ScriptedProvider::new(vec![
            tool_call_reply(vec![("echo", json!({"value": "1"}))]),
            tool_call_reply(vec![("echo", json!({"value": "2"}))]),
        ]));
        let mut tools = ToolRegistry::new();
        tools.register(echo_tool()).unwrap();

        let agent = Agent::new(provider, "test-model")
            .with_tools(tools)
            .with_max_turns(2);
        let err = agent.run(vec![Message::user("go")]).await.unwrap_err();

        assert!(matches!(err, LlmError::MaxTurns { limit: 2 }));
    }

    #[tokio::test]
    async fn registered_tools_are_included_on_every_request() {
        let provider = Arc::new(ScriptedProvider::new(vec![final_reply("hi")]));
        let mut tools = ToolRegistry::new();
        tools.register(echo_tool()).unwrap();

        let agent = Agent::new(provider.clone(), "test-model").with_tools(tools);
        agent.run(vec![Message::user("hello")]).await.unwrap();

        let requests = provider.requests();
        assert_eq!(requests[0].tools.len(), 1);
        assert_eq!(requests[0].tools[0].name, "echo");
    }
}
