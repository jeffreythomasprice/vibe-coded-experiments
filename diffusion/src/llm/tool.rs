use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::llm::LlmError;
use crate::llm::message::Content;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    pub content: Vec<Content>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![Content::text(text)],
        }
    }
}

/// Whether a tool failure means the caller passed bad arguments or the tool's own
/// logic failed, so `ToolRegistry::invoke` can surface each as a distinct `LlmError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolFailureKind {
    InvalidArguments,
    Execution,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolFailure {
    pub message: String,
    pub kind: ToolFailureKind,
}

impl ToolFailure {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToolFailureKind::Execution,
        }
    }

    pub fn invalid_arguments(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            kind: ToolFailureKind::InvalidArguments,
        }
    }
}

impl std::fmt::Display for ToolFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ToolFailure {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolDef {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

/// A tool a `Provider` can call. Implemented directly for MCP-sourced tools later;
/// `FunctionTool` is the adapter for a plain Rust function today.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> &Value;
    async fn invoke(&self, arguments: Value) -> Result<ToolOutput, ToolFailure>;

    fn definition(&self) -> ToolDef {
        ToolDef {
            name: self.name().to_owned(),
            description: self.description().to_owned(),
            schema: self.schema().clone(),
        }
    }
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub struct FunctionTool {
    name: String,
    description: String,
    schema: Value,
    #[allow(clippy::type_complexity)]
    func: Box<dyn Fn(Value) -> BoxFuture<'static, Result<ToolOutput, ToolFailure>> + Send + Sync>,
}

impl FunctionTool {
    /// Wraps a synchronous function taking a `schemars`-derived argument struct.
    /// This is the common case for a plain Rust function tool.
    pub fn sync<A, F>(name: impl Into<String>, description: impl Into<String>, func: F) -> Self
    where
        A: DeserializeOwned + JsonSchema + Send + 'static,
        F: Fn(A) -> Result<ToolOutput, ToolFailure> + Send + Sync + 'static,
    {
        let func = Arc::new(func);
        Self::new::<A, _, _>(name, description, move |args: A| {
            let func = Arc::clone(&func);
            async move { func(args) }
        })
    }

    /// Wraps an async function taking a `schemars`-derived argument struct.
    pub fn new<A, F, Fut>(name: impl Into<String>, description: impl Into<String>, func: F) -> Self
    where
        A: DeserializeOwned + JsonSchema + Send + 'static,
        F: Fn(A) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ToolOutput, ToolFailure>> + Send + 'static,
    {
        Self::with_schema(name, description, argument_schema::<A>(), move |value| {
            let args: A = match serde_json::from_value(value) {
                Ok(args) => args,
                Err(source) => {
                    let fut: BoxFuture<'static, Result<ToolOutput, ToolFailure>> =
                        Box::pin(async move { Err(ToolFailure::invalid_arguments(source.to_string())) });
                    return fut;
                }
            };
            Box::pin(func(args))
        })
    }

    /// Wraps a function with an explicit JSON Schema, bypassing `schemars`. This is
    /// the path an MCP-sourced tool takes, since it arrives with a schema already.
    pub fn with_schema<F>(
        name: impl Into<String>,
        description: impl Into<String>,
        schema: Value,
        func: F,
    ) -> Self
    where
        F: Fn(Value) -> BoxFuture<'static, Result<ToolOutput, ToolFailure>> + Send + Sync + 'static,
    {
        Self {
            name: name.into(),
            description: description.into(),
            schema,
            func: Box::new(func),
        }
    }
}

/// Renders `A`'s JSON Schema, stripping the `$schema`/`title` fields a provider's
/// tool-calling or structured-output API doesn't expect. `pub(crate)` because
/// `eval::tit` reuses it for `ChatOptions::format` rather than a tool argument.
pub(crate) fn argument_schema<A: JsonSchema>() -> Value {
    let schema = schemars::schema_for!(A);
    let mut value = serde_json::to_value(schema).expect("schemars output is always valid JSON");
    if let Value::Object(map) = &mut value {
        map.remove("$schema");
        map.remove("title");
    }
    value
}

#[async_trait]
impl Tool for FunctionTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn schema(&self) -> &Value {
        &self.schema
    }

    async fn invoke(&self, arguments: Value) -> Result<ToolOutput, ToolFailure> {
        (self.func)(arguments).await
    }
}

/// Tools available to an `Agent`, keyed by name in a `BTreeMap` for stable ordering
/// in `definitions()` (and so a provider always sees tools in the same order).
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: impl Tool + 'static) -> Result<(), LlmError> {
        let name = tool.name().to_owned();
        if self.tools.contains_key(&name) {
            return Err(LlmError::DuplicateTool { name });
        }
        self.tools.insert(name, Arc::new(tool));
        Ok(())
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn definitions(&self) -> Vec<ToolDef> {
        self.tools.values().map(|tool| tool.definition()).collect()
    }

    pub async fn invoke(&self, call: &ToolCall) -> Result<ToolOutput, LlmError> {
        let Some(tool) = self.tools.get(&call.name) else {
            let available = if self.tools.is_empty() {
                "none".to_owned()
            } else {
                self.tools.keys().cloned().collect::<Vec<_>>().join(", ")
            };
            return Err(LlmError::UnknownTool {
                name: call.name.clone(),
                available,
            });
        };
        tool.invoke(call.arguments.clone())
            .await
            .map_err(|source| match source.kind {
                ToolFailureKind::InvalidArguments => LlmError::ToolArguments {
                    tool: call.name.clone(),
                    message: source.message,
                },
                ToolFailureKind::Execution => LlmError::ToolFailed {
                    tool: call.name.clone(),
                    message: source.message,
                },
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, JsonSchema)]
    struct Weather {
        city: String,
        #[allow(dead_code)]
        days: Option<u8>,
    }

    fn weather_tool() -> FunctionTool {
        FunctionTool::sync::<Weather, _>("get_weather", "Look up a forecast", |args| {
            Ok(ToolOutput::text(format!("forecast for {}", args.city)))
        })
    }

    #[test]
    fn derived_schema_has_expected_shape_and_no_metadata() {
        let tool = weather_tool();
        let schema = tool.schema();
        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
        assert_eq!(schema["properties"]["city"]["type"], "string");
        let required = schema["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == "city"));
        assert!(!required.iter().any(|v| v == "days"));
    }

    #[tokio::test]
    async fn sync_tool_runs_and_returns_text() {
        let tool = weather_tool();
        let output = tool
            .invoke(serde_json::json!({"city": "Boston"}))
            .await
            .unwrap();
        assert_eq!(output, ToolOutput::text("forecast for Boston"));
    }

    #[tokio::test]
    async fn malformed_arguments_fail_the_tool_not_the_process() {
        let tool = weather_tool();
        let err = tool.invoke(serde_json::json!({"days": 3})).await.unwrap_err();
        assert_eq!(err.kind, ToolFailureKind::InvalidArguments);
    }

    #[tokio::test]
    async fn malformed_arguments_surface_as_tool_arguments_error() {
        let mut registry = ToolRegistry::new();
        registry.register(weather_tool()).unwrap();

        let call = ToolCall {
            id: "call_0".to_owned(),
            name: "get_weather".to_owned(),
            arguments: serde_json::json!({"days": 3}),
        };
        let err = registry.invoke(&call).await.unwrap_err();
        assert!(matches!(err, LlmError::ToolArguments { tool, .. } if tool == "get_weather"));
    }

    #[test]
    fn duplicate_registration_is_rejected() {
        let mut registry = ToolRegistry::new();
        registry.register(weather_tool()).unwrap();
        let err = registry.register(weather_tool()).unwrap_err();
        assert!(matches!(err, LlmError::DuplicateTool { name } if name == "get_weather"));
    }

    #[tokio::test]
    async fn unknown_tool_lists_available_names() {
        let mut registry = ToolRegistry::new();
        registry.register(weather_tool()).unwrap();

        let call = ToolCall {
            id: "call_0".to_owned(),
            name: "nonexistent".to_owned(),
            arguments: serde_json::json!({}),
        };
        let err = registry.invoke(&call).await.unwrap_err();
        match err {
            LlmError::UnknownTool { name, available } => {
                assert_eq!(name, "nonexistent");
                assert_eq!(available, "get_weather");
            }
            other => panic!("expected UnknownTool, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn definitions_are_stably_ordered() {
        let mut registry = ToolRegistry::new();
        registry
            .register(FunctionTool::sync::<Weather, _>("zzz_tool", "z", |_: Weather| {
                Ok(ToolOutput::text("z"))
            }))
            .unwrap();
        registry.register(weather_tool()).unwrap();

        let names: Vec<_> = registry.definitions().into_iter().map(|d| d.name).collect();
        assert_eq!(names, vec!["get_weather", "zzz_tool"]);
    }

    #[tokio::test]
    async fn tool_error_is_wrapped_with_tool_name() {
        let mut registry = ToolRegistry::new();
        registry
            .register(FunctionTool::sync::<Weather, _>("failer", "always fails", |_: Weather| {
                Err(ToolFailure::new("boom"))
            }))
            .unwrap();

        let call = ToolCall {
            id: "call_0".to_owned(),
            name: "failer".to_owned(),
            arguments: serde_json::json!({"city": "x"}),
        };
        let err = registry.invoke(&call).await.unwrap_err();
        match err {
            LlmError::ToolFailed { tool, message } => {
                assert_eq!(tool, "failer");
                assert_eq!(message, "boom");
            }
            other => panic!("expected ToolFailed, got {other:?}"),
        }
    }
}
