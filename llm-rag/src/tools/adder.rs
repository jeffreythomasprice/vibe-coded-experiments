// TODO: remove — temporary scaffolding to exercise tool-calling end-to-end;
// delete once real tools land.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};

use super::{ToolError, ToolImpl};
use crate::llm::types::Tool;

const NAME: &str = "add";

pub struct AdderTool;

#[derive(Deserialize)]
struct AdderArgs {
    a: f64,
    b: f64,
}

#[async_trait]
impl ToolImpl for AdderTool {
    fn definition(&self) -> Tool {
        Tool {
            name: NAME.into(),
            description: "adds numbers".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "a": { "type": "number" },
                    "b": { "type": "number" },
                },
                "required": ["a", "b"],
            }),
        }
    }

    async fn invoke(&self, args: Value) -> Result<String, ToolError> {
        let parsed: AdderArgs =
            serde_json::from_value(args).map_err(|e| ToolError::InvalidArguments {
                tool: NAME,
                source: e.into(),
            })?;
        Ok((parsed.a + parsed.b).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn definition_has_expected_shape() {
        let def = AdderTool.definition();
        assert_eq!(def.name, "add");
        assert_eq!(def.description, "adds numbers");
        let required = def.parameters.get("required").unwrap();
        assert!(required.as_array().unwrap().iter().any(|v| v == "a"));
        assert!(required.as_array().unwrap().iter().any(|v| v == "b"));
    }

    #[tokio::test]
    async fn invoke_adds_two_numbers() {
        assert_eq!(
            AdderTool.invoke(json!({"a": 17, "b": 25})).await.unwrap(),
            "42"
        );
    }

    #[tokio::test]
    async fn invoke_rejects_missing_argument() {
        let err = AdderTool.invoke(json!({"a": 1})).await.unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments { .. }));
    }

    #[tokio::test]
    async fn invoke_rejects_wrong_type() {
        let err = AdderTool
            .invoke(json!({"a": "two", "b": 3}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidArguments { .. }));
    }
}
