use std::sync::Mutex;

use async_trait::async_trait;

use crate::llm::LlmError;
use crate::llm::provider::{ChatRequest, ChatResponse, Provider};

/// A `Provider` that returns a fixed queue of responses in order and records every
/// request it received, so a test can assert on exactly what the agent loop sent —
/// this is the "real provider mocked" the unit tests stand in for.
pub struct ScriptedProvider {
    responses: Mutex<Vec<ChatResponse>>,
    requests: Mutex<Vec<ChatRequest>>,
}

impl ScriptedProvider {
    pub fn new(responses: Vec<ChatResponse>) -> Self {
        Self {
            responses: Mutex::new(responses.into_iter().rev().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    pub fn requests(&self) -> Vec<ChatRequest> {
        self.requests.lock().unwrap().clone()
    }
}

#[async_trait]
impl Provider for ScriptedProvider {
    fn name(&self) -> &'static str {
        "scripted"
    }

    async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(self
            .responses
            .lock()
            .unwrap()
            .pop()
            .expect("ScriptedProvider ran out of queued responses"))
    }
}
