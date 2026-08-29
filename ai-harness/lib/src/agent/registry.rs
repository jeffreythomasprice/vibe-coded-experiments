//! The process-wide catalog of tools this build knows how to execute, and
//! the bridge from a stored `shared::agent::AgentConfig` (which names tools)
//! to an executable [`Agent`] (which needs `Arc<dyn Tool>`).
//!
//! `AgentBuilder::build_with` overwrites `AgentSpec::tools` from its
//! `ToolBox`, so a stored config can't carry executable tools directly —
//! only names, resolved here against whatever this build has registered.

use std::collections::BTreeMap;
use std::sync::Arc;

use shared::agent::{AgentConfig, ToolSpec};

use crate::llm::router::Router;

use super::error::AgentError;
use super::tool::{Tool, ToolContext};
use super::Agent;

/// Tools this build can execute, by name.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: impl Tool + 'static) {
        self.register_arc(Arc::new(tool));
    }

    pub fn register_arc(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.def().name.clone(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    /// Every tool this build knows, with its own default approval — what a
    /// "pick your tools" UI lists.
    pub fn catalog(&self) -> Vec<ToolSpec> {
        self.tools
            .values()
            .map(|t| ToolSpec {
                def: t.def().clone(),
                approval: t.approval(),
            })
            .collect()
    }

    /// Resolve a stored config's tool selection into executable tools.
    /// Errors on an unknown name rather than silently dropping it — a config
    /// naming a tool this build no longer has would otherwise quietly
    /// downgrade the agent, with nothing in the UI to explain why.
    pub fn resolve(&self, selection: &[String]) -> Result<Vec<Arc<dyn Tool>>, AgentError> {
        selection
            .iter()
            .map(|name| {
                self.get(name).ok_or_else(|| AgentError::UnknownTool {
                    name: name.clone(),
                    available: self.names(),
                })
            })
            .collect()
    }
}

/// Build a runnable [`Agent`] from a stored config, resolving its tool
/// selection through `registry` first.
///
/// `AgentBuilder::build_with` overwrites `AgentSpec::tools` with
/// `tool_box.specs()` — which is exactly what's wanted here, since a config
/// only ever *selects* tools by name and can't re-gate them: the `AgentSpec`
/// a caller reads back off the built `Agent` always reports each tool's own
/// approval.
///
/// `ctx` is what every tool call in this agent's turns actually runs
/// against — its virtual filesystem and sandbox backend — built fresh per
/// turn by `lib::service::chat` from the conversation's live project mounts,
/// never cached on the registry (mounts are a live grant, not a premise; see
/// `shared::project`'s module doc).
pub fn build_agent(
    config: &AgentConfig,
    registry: &ToolRegistry,
    router: &Router,
    ctx: ToolContext,
) -> Result<Agent, AgentError> {
    let tools = registry.resolve(&config.input.tools)?;
    let mut builder = Agent::builder(config.input.model.clone(), config.input.max_tokens)
        .max_steps(config.input.max_steps)
        .thinking(config.input.thinking.clone())
        .context(ctx)
        .tools(tools);
    for piece in &config.input.system {
        builder = builder.system(piece.clone());
    }
    if let Some(choice) = &config.input.tool_choice {
        builder = builder.tool_choice(choice.clone());
    }
    for sequence in &config.input.stop_sequences {
        builder = builder.stop_sequence(sequence.clone());
    }
    builder.build(router)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tool::{tool, JsonSchema};
    use crate::llm::router::{ProviderEntry, Router};
    use crate::llm::testing::ScriptedProvider;
    use serde::Deserialize;
    use shared::agent::{AgentConfigInput, Approval};
    use shared::ids::AgentConfigId;
    use shared::llm::message::{CompletedMessage, ContentBlock, Role, StopReason};
    use shared::llm::model::ModelRef;
    use shared::llm::tool::Thinking;

    #[derive(Deserialize, JsonSchema)]
    struct PingArgs {}

    fn ping_tool() -> impl Tool {
        tool("ping", "Ping", |_args: PingArgs| async move {
            Ok::<_, anyhow::Error>(crate::agent::tool::ToolOutput::from("pong"))
        })
    }

    fn gated_ping_tool() -> impl Tool {
        tool("ping", "Ping", |_args: PingArgs| async move {
            Ok::<_, anyhow::Error>(crate::agent::tool::ToolOutput::from("pong"))
        })
        .requiring_approval()
    }

    #[test]
    fn resolve_errors_on_an_unknown_tool_and_lists_whats_available() {
        let mut registry = ToolRegistry::new();
        registry.register(ping_tool());
        let Err(err) = registry.resolve(&["deploy".to_string()]) else {
            panic!("expected resolve to fail on an unregistered tool name");
        };
        match err {
            AgentError::UnknownTool { name, available } => {
                assert_eq!(name, "deploy");
                assert_eq!(available, vec!["ping".to_string()]);
            }
            other => panic!("expected UnknownTool, got {other:?}"),
        }
    }

    #[test]
    fn resolve_reports_each_tools_own_approval_since_a_selection_cant_change_it() {
        let mut registry = ToolRegistry::new();
        registry.register(gated_ping_tool());
        let resolved = registry.resolve(&["ping".to_string()]).unwrap();
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].approval(), Approval::RequiresApproval);
        assert_eq!(resolved[0].def().name, "ping");
    }

    #[test]
    fn catalog_reports_every_registered_tool_with_its_default_approval() {
        let mut registry = ToolRegistry::new();
        registry.register(ping_tool());
        let catalog = registry.catalog();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].def.name, "ping");
        assert_eq!(catalog[0].approval, Approval::Automatic);
    }

    fn agent_config(tools: Vec<String>) -> AgentConfig {
        AgentConfig {
            id: AgentConfigId(1),
            input: AgentConfigInput {
                name: "ops".to_string(),
                description: None,
                model: ModelRef::new("scripted", "test-model"),
                system: vec!["Be terse.".to_string()],
                max_tokens: 256,
                tools,
                tool_choice: None,
                thinking: Thinking::default(),
                stop_sequences: vec![],
                max_steps: 4,
            },
            created_at: "2026-08-22T00:00:00.000Z".to_string(),
            updated_at: "2026-08-22T00:00:00.000Z".to_string(),
        }
    }

    #[test]
    fn build_agent_reports_the_tools_own_approval_a_selection_cant_downgrade_it() {
        let mut registry = ToolRegistry::new();
        registry.register(gated_ping_tool());

        let scripted = ScriptedProvider::new(vec![Ok(CompletedMessage {
            role: Role::Assistant,
            content: vec![ContentBlock::Text {
                text: "hi".to_string(),
            }],
            stop_reason: StopReason::EndTurn,
            usage: Default::default(),
            model: Some("test-model".to_string()),
        })]);
        let mut router = Router::new();
        router.register(
            "scripted",
            ProviderEntry {
                chat: Some(Arc::new(scripted)),
                ..Default::default()
            },
        );

        // The config names the tool, nothing more — there's no way for it to
        // ask for anything but the tool's own approval.
        let config = agent_config(vec!["ping".to_string()]);
        let agent = build_agent(&config, &registry, &router, ToolContext::default()).unwrap();
        assert_eq!(agent.spec().tools.len(), 1);
        assert_eq!(agent.spec().tools[0].approval, Approval::RequiresApproval);
        assert_eq!(agent.spec().max_steps, 4);
        assert_eq!(agent.spec().system_prompt().as_deref(), Some("Be terse."));
    }

    #[test]
    fn build_agent_errors_on_an_unknown_tool_before_touching_the_router() {
        let registry = ToolRegistry::new();
        let router = Router::new();
        let config = agent_config(vec!["does-not-exist".to_string()]);
        let Err(err) = build_agent(&config, &registry, &router, ToolContext::default()) else {
            panic!("expected build_agent to fail on an unregistered tool name");
        };
        assert!(matches!(err, AgentError::UnknownTool { .. }));
    }
}
