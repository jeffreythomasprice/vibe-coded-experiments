//! Server-side tool registry and dispatcher.
//!
//! Each tool implements [`ToolImpl`] and is registered in [`ToolRegistry`].
//! The chat handler queries the registry for [`Tool`] definitions to offer the
//! LLM, and dispatches by name when the model emits a tool call.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;
use thiserror::Error;

use crate::llm::types::Tool;

pub mod adder;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("tool `{tool}` rejected arguments: {source}")]
    InvalidArguments {
        tool: &'static str,
        #[source]
        source: anyhow::Error,
    },
    #[error("tool `{tool}` execution failed: {source}")]
    Execution {
        tool: &'static str,
        #[source]
        source: anyhow::Error,
    },
    #[error("no such tool: `{0}`")]
    Unknown(String),
}

#[async_trait]
pub trait ToolImpl: Send + Sync {
    fn definition(&self) -> Tool;
    async fn invoke(&self, args: Value) -> Result<String, ToolError>;
}

#[derive(Default)]
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn ToolImpl>>,
}

impl ToolRegistry {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Registry preloaded with every tool shipped by the binary today.
    pub fn with_defaults() -> Self {
        let mut reg = Self::empty();
        reg.register(Arc::new(adder::AdderTool));
        reg
    }

    pub fn register(&mut self, tool: Arc<dyn ToolImpl>) {
        let name = tool.definition().name;
        self.tools.insert(name, tool);
    }

    pub fn definitions(&self) -> Vec<Tool> {
        self.tools.values().map(|t| t.definition()).collect()
    }

    pub async fn invoke(&self, name: &str, args: Value) -> Result<String, ToolError> {
        match self.tools.get(name) {
            Some(t) => t.invoke(args).await,
            None => Err(ToolError::Unknown(name.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn with_defaults_contains_adder() {
        let reg = ToolRegistry::with_defaults();
        let names: Vec<String> = reg.definitions().into_iter().map(|t| t.name).collect();
        assert!(names.contains(&"add".to_string()));
    }

    #[tokio::test]
    async fn invoke_adder_via_registry() {
        let reg = ToolRegistry::with_defaults();
        let out = reg
            .invoke("add", serde_json::json!({"a": 2.5, "b": 1.5}))
            .await
            .unwrap();
        assert_eq!(out, "4");
    }

    #[tokio::test]
    async fn invoke_unknown_tool_errors() {
        let reg = ToolRegistry::with_defaults();
        let err = reg.invoke("nope", serde_json::json!({})).await.unwrap_err();
        assert!(matches!(err, ToolError::Unknown(_)));
    }
}
