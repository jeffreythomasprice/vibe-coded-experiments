//! The generic tool concept: something an [`crate::agent::Agent`] can call by
//! name with JSON input and get [`ToolResultContent`] back.
//!
//! Three ways to make one, in increasing order of how much you write by
//! hand:
//!
//! 1. Implement [`Tool`] directly — full control, e.g. over `approval()`.
//! 2. [`tool`] — a plain Rust fn, its argument type derives
//!    [`JsonSchema`] (re-exported here from `schemars`), and its JSON Schema
//!    is generated for you. The ergonomic default.
//! 3. [`json_tool`] — a hand-written `serde_json::Value` schema, for a tool
//!    built at runtime (from config, from a remote listing) where there is no
//!    Rust type to derive from.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use async_trait::async_trait;
use serde::de::DeserializeOwned;
use shared::agent::Approval;
use shared::llm::{ImageSource, ToolDef, ToolResultContent};

pub use schemars::JsonSchema;

use crate::agent::error::AgentError;
use crate::sandbox::SandboxBackend;
use crate::vfs::{MountTable, Vfs};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Tunable limits a tool consults at call time — how long a `bash` call may
/// run, how much of its output to keep, how large a file `read_file` will
/// read. Populated from `[tools]` in `lib::config` (see `lib::agent::builtin`);
/// `Default` gives every existing test fixture and `ToolContext::default()`
/// caller a sensible value with no config in scope.
#[derive(Debug, Clone, Copy)]
pub struct ToolLimits {
    pub bash_timeout_secs: u64,
    pub max_output_bytes: usize,
    pub max_read_bytes: usize,
    /// Whether a sandboxed subprocess may reach the network — plumbed from
    /// `config.sandbox.network`, which nothing reads today; see
    /// `lib::agent::builtin::bash`'s module doc.
    pub network: bool,
}

impl Default for ToolLimits {
    fn default() -> Self {
        Self {
            bash_timeout_secs: 120,
            max_output_bytes: 30_000,
            max_read_bytes: 262_144,
            network: true,
        }
    }
}

struct ToolContextInner {
    vfs: Vfs,
    sandbox: Arc<dyn SandboxBackend>,
    sandbox_available: bool,
    limits: ToolLimits,
}

/// What a tool needs to actually do its work: the calling conversation's
/// virtual filesystem, the sandbox backend to run a confined subprocess
/// under, and the configured limits — everything a process-wide
/// [`ToolRegistry`](super::registry::ToolRegistry) entry can't carry itself,
/// because mounts are per-conversation and re-resolved live on every use
/// (see `shared::project`'s module doc: a project is a *grant*, not a
/// premise). Built once per turn by `lib::service::chat`, not once per call.
///
/// Cheap to clone — one `Arc` — so a handler that outlives the call that
/// created it (an `FnTool`'s `'static` future) can own one.
#[derive(Clone)]
pub struct ToolContext(Arc<ToolContextInner>);

impl ToolContext {
    pub fn new(vfs: Vfs, sandbox: Arc<dyn SandboxBackend>, sandbox_available: bool, limits: ToolLimits) -> Self {
        Self(Arc::new(ToolContextInner {
            vfs,
            sandbox,
            sandbox_available,
            limits,
        }))
    }

    pub fn vfs(&self) -> &Vfs {
        &self.0.vfs
    }

    pub fn sandbox(&self) -> &Arc<dyn SandboxBackend> {
        &self.0.sandbox
    }

    /// Whether the sandbox backend actually works on this machine — see
    /// `lib::sandbox::Availability`. A tool that needs to run a subprocess
    /// must check this and fail closed (an `is_error` result, never an
    /// unconfined run) rather than trusting `sandbox()` alone; `Disabled`
    /// already refuses every call, but checking here gives a clearer message.
    pub fn sandbox_available(&self) -> bool {
        self.0.sandbox_available
    }

    pub fn limits(&self) -> ToolLimits {
        self.0.limits
    }
}

impl Default for ToolContext {
    /// An empty, no-access `MountTable` and a `Disabled` sandbox — what every
    /// existing test fixture and `AgentBuilder` gets until a caller supplies
    /// a real one via `AgentBuilder::context`.
    fn default() -> Self {
        Self::new(
            Vfs::new(MountTable::default()),
            Arc::new(crate::sandbox::Disabled::new(
                "no ToolContext configured for this agent".to_string(),
            )),
            false,
            ToolLimits::default(),
        )
    }
}

/// Something an agent can call by name with JSON input.
#[async_trait]
pub trait Tool: Send + Sync {
    fn def(&self) -> &ToolDef;

    /// Whether a call must be approved by the user before it runs. Defaults
    /// to [`Approval::Automatic`] — most tools don't need a gate.
    fn approval(&self) -> Approval {
        Approval::Automatic
    }

    /// Guidance this tool contributes to its agent's system prompt — how and
    /// when to use it. `None` (the default) contributes nothing; see
    /// [`ToolBox::system_section`] for how these are assembled.
    fn system_prompt(&self) -> Option<String> {
        None
    }

    /// `Err` is *not* an agent failure — the loop turns it into a
    /// `tool_result` with `is_error: true` that the model gets to see and
    /// react to, the same as a malformed name or malformed input. `anyhow`,
    /// not a `thiserror` enum: nothing downstream ever matches on the
    /// specific failure, only on whether there was one — see
    /// `lib::llm::error`'s module doc for when this repo reaches for
    /// `thiserror` instead.
    async fn call(&self, ctx: &ToolContext, input: serde_json::Value) -> anyhow::Result<ToolOutput>;
}

/// What a tool call produced, on success.
#[derive(Debug)]
pub struct ToolOutput {
    pub content: Vec<ToolResultContent>,
}

impl ToolOutput {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            content: vec![ToolResultContent::Text { text: text.into() }],
        }
    }

    pub fn image(source: ImageSource) -> Self {
        Self {
            content: vec![ToolResultContent::Image { source }],
        }
    }
}

impl From<String> for ToolOutput {
    fn from(text: String) -> Self {
        ToolOutput::text(text)
    }
}

impl From<&str> for ToolOutput {
    fn from(text: &str) -> Self {
        ToolOutput::text(text)
    }
}

/// Normalizes `schemars`' raw output into something safe to hand a provider:
/// no `$schema` (redundant — every `ToolDef.input_schema` is already known to
/// be JSON Schema), no `title` (schemars derives it from the Rust type name,
/// which is an implementation detail no provider needs), and no
/// `$defs`/`$ref` — nested types are inlined into one self-contained object,
/// the safest shape across all three providers and especially for the small
/// local models Ollama serves, which are the least reliable at resolving a
/// `$ref`.
pub fn schema_for<T: JsonSchema>() -> serde_json::Value {
    let mut value = schemars::generate::SchemaSettings::draft2020_12()
        .with(|s| {
            s.meta_schema = None;
            s.inline_subschemas = true;
        })
        .into_generator()
        .into_root_schema_for::<T>()
        .to_value();
    if let Some(object) = value.as_object_mut() {
        object.remove("title");
    }
    value
}

/// A [`Tool`] built from a plain async closure, via [`tool`], [`json_tool`],
/// or [`ctx_tool`].
pub struct FnTool {
    def: ToolDef,
    approval: Approval,
    system_prompt: Option<String>,
    #[allow(clippy::type_complexity)]
    handler: Box<
        dyn Fn(ToolContext, serde_json::Value) -> BoxFuture<'static, anyhow::Result<ToolOutput>>
            + Send
            + Sync,
    >,
}

impl FnTool {
    /// Consuming-self opt-in setter, matching `AnthropicClient::with_cache`'s
    /// pattern elsewhere in this crate — a tool is automatic unless you say
    /// otherwise.
    pub fn requiring_approval(mut self) -> Self {
        self.approval = Approval::RequiresApproval;
        self
    }

    /// Guidance this tool contributes to its agent's system prompt — see
    /// [`Tool::system_prompt`].
    pub fn with_system_prompt(mut self, text: impl Into<String>) -> Self {
        self.system_prompt = Some(text.into());
        self
    }
}

#[async_trait]
impl Tool for FnTool {
    fn def(&self) -> &ToolDef {
        &self.def
    }

    fn approval(&self) -> Approval {
        self.approval
    }

    fn system_prompt(&self) -> Option<String> {
        self.system_prompt.clone()
    }

    async fn call(&self, ctx: &ToolContext, input: serde_json::Value) -> anyhow::Result<ToolOutput> {
        (self.handler)(ctx.clone(), input).await
    }
}

/// Build a tool from a plain Rust fn that doesn't need a [`ToolContext`]. `A`'s
/// JSON Schema is generated by [`schema_for`]; the model's raw JSON input is
/// deserialized into `A` before `handler` ever sees it — a deserialize
/// failure becomes an ordinary `Err`, which the agent loop turns into an
/// `is_error` tool result rather than aborting the turn.
pub fn tool<A, F, Fut, R>(
    name: impl Into<String>,
    description: impl Into<String>,
    handler: F,
) -> FnTool
where
    A: DeserializeOwned + JsonSchema + Send + 'static,
    F: Fn(A) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<R>> + Send + 'static,
    R: Into<ToolOutput>,
{
    ctx_tool(name, description, move |_ctx, args| handler(args))
}

/// Build a tool from a hand-written JSON Schema, for a tool assembled at
/// runtime with no Rust type to derive one from. `handler` receives the raw
/// `serde_json::Value` — nothing is deserialized on its behalf. Doesn't take
/// a [`ToolContext`] either, for the same reason [`tool`] doesn't — a
/// context-needing runtime-schema tool has no constructor yet, since none of
/// this crate's tools need both at once.
pub fn json_tool<F, Fut, R>(
    name: impl Into<String>,
    description: impl Into<String>,
    input_schema: serde_json::Value,
    handler: F,
) -> FnTool
where
    F: Fn(serde_json::Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<R>> + Send + 'static,
    R: Into<ToolOutput>,
{
    let handler = Arc::new(handler);
    FnTool {
        def: ToolDef {
            name: name.into(),
            description: description.into(),
            input_schema,
        },
        approval: Approval::Automatic,
        system_prompt: None,
        handler: Box::new(move |_ctx, value| {
            let handler = handler.clone();
            Box::pin(async move { Ok(handler(value).await?.into()) })
        }),
    }
}

/// Build a tool from a plain Rust fn that needs the calling conversation's
/// [`ToolContext`] — a filesystem or sandbox tool, for instance. Otherwise
/// identical to [`tool`]; see that function's doc.
pub fn ctx_tool<A, F, Fut, R>(
    name: impl Into<String>,
    description: impl Into<String>,
    handler: F,
) -> FnTool
where
    A: DeserializeOwned + JsonSchema + Send + 'static,
    F: Fn(ToolContext, A) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<R>> + Send + 'static,
    R: Into<ToolOutput>,
{
    let handler = Arc::new(handler);
    FnTool {
        def: ToolDef {
            name: name.into(),
            description: description.into(),
            input_schema: schema_for::<A>(),
        },
        approval: Approval::Automatic,
        system_prompt: None,
        handler: Box::new(move |ctx, value| {
            let handler = handler.clone();
            Box::pin(async move {
                let args: A = serde_json::from_value(value)?;
                Ok(handler(ctx, args).await?.into())
            })
        }),
    }
}

/// The tools one agent knows about, keyed by name. Ordered (`BTreeMap`) so
/// [`ToolBox::defs`]/[`ToolBox::specs`] have a stable, deterministic order
/// regardless of registration order.
#[derive(Default)]
pub struct ToolBox {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolBox {
    pub fn new() -> Self {
        Self::default()
    }

    /// Errors if a tool with the same name is already registered — a
    /// silent overwrite would mean the model's declared tool list and the
    /// tool that actually runs could disagree about which one it's calling.
    pub fn add(&mut self, tool: impl Tool + 'static) -> Result<(), AgentError> {
        self.add_arc(Arc::new(tool))
    }

    /// Same as [`ToolBox::add`], for a tool that's already behind an `Arc` —
    /// what `crate::agent::AgentBuilder::build_with` uses, since its own
    /// tools accumulate as `Arc<dyn Tool>` from the start.
    pub fn add_arc(&mut self, tool: Arc<dyn Tool>) -> Result<(), AgentError> {
        let name = tool.def().name.clone();
        if self.tools.contains_key(&name) {
            return Err(AgentError::DuplicateTool { name });
        }
        self.tools.insert(name, tool);
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Every tool's declaration, for `ChatOptions.tools`.
    pub fn defs(&self) -> Vec<ToolDef> {
        self.tools.values().map(|t| t.def().clone()).collect()
    }

    /// Every tool's declaration plus its approval policy, for
    /// `shared::agent::AgentSpec.tools` — derived from the same tools that
    /// actually run, so the serializable spec can never drift from what
    /// `Agent::next_turn` does.
    pub fn specs(&self) -> Vec<shared::agent::ToolSpec> {
        self.tools
            .values()
            .map(|t| shared::agent::ToolSpec {
                def: t.def().clone(),
                approval: t.approval(),
            })
            .collect()
    }

    /// One system-prompt block covering every tool that contributes
    /// [`Tool::system_prompt`] guidance, under a header that makes it
    /// unambiguous to the model that this section is about *how* to use its
    /// tools, not part of the agent's own persona. `None` when no tool has
    /// anything to say — an agent with no tools, or only ones with no
    /// guidance of their own, gets no such section at all.
    pub fn system_section(&self) -> Option<String> {
        let sections: Vec<(String, String)> = self
            .tools
            .values()
            .filter_map(|t| t.system_prompt().map(|prompt| (t.def().name.clone(), prompt)))
            .collect();
        if sections.is_empty() {
            return None;
        }
        let mut text = String::from(
            "# Tools\n\nYou have access to the tools below. Use them when they help you \
             answer the request; a tool marked as requiring approval will pause your turn \
             until the user approves or denies it — that is expected, not an error.",
        );
        for (name, prompt) in sections {
            text.push_str(&format!("\n\n## {name}\n{prompt}"));
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, JsonSchema)]
    struct WeatherArgs {
        city: String,
    }

    // Only used for their derived JsonSchema/Deserialize impls below — never
    // constructed or field-accessed directly, hence `allow(dead_code)`.
    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct Inner {
        value: i32,
    }

    #[allow(dead_code)]
    #[derive(Debug, Deserialize, JsonSchema)]
    struct Outer {
        inner: Inner,
        name: String,
    }

    #[test]
    fn schema_for_drops_schema_and_title_and_keeps_properties() {
        let schema = schema_for::<WeatherArgs>();
        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
        assert!(schema["properties"]["city"].is_object());
        assert_eq!(schema["required"], serde_json::json!(["city"]));
    }

    #[test]
    fn schema_for_inlines_nested_types_instead_of_using_refs() {
        let schema = schema_for::<Outer>();
        assert!(
            schema.get("$defs").is_none(),
            "expected no $defs, got {schema}"
        );
        let inner = &schema["properties"]["inner"];
        assert!(
            inner.get("$ref").is_none(),
            "expected the nested type inlined, not a $ref: {inner}"
        );
        assert!(inner["properties"]["value"].is_object());
    }

    #[tokio::test]
    async fn tool_deserializes_valid_input_and_runs_the_handler() {
        let weather = tool(
            "get_weather",
            "Look up the weather",
            |args: WeatherArgs| async move { Ok(format!("72F and sunny in {}", args.city)) },
        );
        let output = weather
            .call(&ToolContext::default(), serde_json::json!({"city": "Paris"}))
            .await
            .unwrap();
        match &output.content[0] {
            ToolResultContent::Text { text } => assert_eq!(text, "72F and sunny in Paris"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn tool_call_fails_when_input_does_not_match_the_schema() {
        let weather = tool(
            "get_weather",
            "Look up the weather",
            |args: WeatherArgs| async move { Ok(format!("72F in {}", args.city)) },
        );
        let err = weather
            .call(&ToolContext::default(), serde_json::json!({"not_city": "Paris"}))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("city"),
            "expected the deserialize error to name the missing field, got: {err}"
        );
    }

    #[test]
    fn tool_defaults_to_automatic_and_requiring_approval_flips_it() {
        let automatic = tool(
            "noop",
            "does nothing",
            |_: WeatherArgs| async move { Ok("ok") },
        );
        assert_eq!(automatic.approval(), Approval::Automatic);

        let gated = tool("deploy", "ship to prod", |_: WeatherArgs| async move {
            Ok("ok")
        })
        .requiring_approval();
        assert_eq!(gated.approval(), Approval::RequiresApproval);
    }

    #[tokio::test]
    async fn json_tool_hands_the_handler_raw_json_with_no_deserialization() {
        let echo = json_tool(
            "echo",
            "echoes its input",
            serde_json::json!({"type": "object"}),
            |value: serde_json::Value| async move { Ok(value.to_string()) },
        );
        let output = echo
            .call(&ToolContext::default(), serde_json::json!({"a": 1}))
            .await
            .unwrap();
        match &output.content[0] {
            ToolResultContent::Text { text } => assert_eq!(text, r#"{"a":1}"#),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn tool_output_from_str_and_string_both_produce_text() {
        let from_str: ToolOutput = "hello".into();
        let from_string: ToolOutput = "hello".to_string().into();
        for output in [from_str, from_string] {
            match &output.content[0] {
                ToolResultContent::Text { text } => assert_eq!(text, "hello"),
                other => panic!("expected Text, got {other:?}"),
            }
        }
    }

    #[test]
    fn tool_box_add_rejects_a_duplicate_name() {
        let mut tools = ToolBox::new();
        tools
            .add(tool(
                "ping",
                "ping",
                |_: WeatherArgs| async move { Ok("pong") },
            ))
            .unwrap();
        let err = tools
            .add(tool("ping", "ping again", |_: WeatherArgs| async move {
                Ok("pong")
            }))
            .unwrap_err();
        assert!(matches!(err, AgentError::DuplicateTool { name } if name == "ping"));
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn tool_box_defs_and_specs_are_in_a_stable_alphabetical_order() {
        let mut tools = ToolBox::new();
        tools
            .add(tool("zeta", "z", |_: WeatherArgs| async move { Ok("z") }))
            .unwrap();
        tools
            .add(tool("alpha", "a", |_: WeatherArgs| async move { Ok("a") }).requiring_approval())
            .unwrap();

        let defs = tools.defs();
        assert_eq!(
            defs.iter().map(|d| d.name.as_str()).collect::<Vec<_>>(),
            vec!["alpha", "zeta"]
        );

        let specs = tools.specs();
        assert_eq!(specs[0].def.name, "alpha");
        assert_eq!(specs[0].approval, Approval::RequiresApproval);
        assert_eq!(specs[1].def.name, "zeta");
        assert_eq!(specs[1].approval, Approval::Automatic);
    }

    #[test]
    fn tool_box_get_returns_a_clone_of_the_registered_tool() {
        let mut tools = ToolBox::new();
        tools
            .add(tool(
                "ping",
                "ping",
                |_: WeatherArgs| async move { Ok("pong") },
            ))
            .unwrap();
        assert!(tools.get("ping").is_some());
        assert!(tools.get("missing").is_none());
    }

    #[test]
    fn tool_context_default_has_no_filesystem_access_and_reports_the_sandbox_unavailable() {
        let ctx = ToolContext::default();
        assert!(ctx.vfs().table().is_empty());
        assert!(!ctx.sandbox_available());
    }

    #[tokio::test]
    async fn ctx_tool_hands_the_handler_the_calling_context() {
        let echo = ctx_tool(
            "context_probe",
            "reports whether the sandbox looked available",
            |ctx: ToolContext, _args: WeatherArgs| async move {
                Ok::<_, anyhow::Error>(ctx.sandbox_available().to_string())
            },
        );
        let output = echo
            .call(&ToolContext::default(), serde_json::json!({"city": "Paris"}))
            .await
            .unwrap();
        match &output.content[0] {
            ToolResultContent::Text { text } => assert_eq!(text, "false"),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn with_system_prompt_is_reported_by_the_tool_and_absent_by_default() {
        let plain = tool("noop", "does nothing", |_: WeatherArgs| async move { Ok("ok") });
        assert!(plain.system_prompt().is_none());

        let documented = tool("noop", "does nothing", |_: WeatherArgs| async move { Ok("ok") })
            .with_system_prompt("Call this whenever you need to do nothing.");
        assert_eq!(
            documented.system_prompt().as_deref(),
            Some("Call this whenever you need to do nothing.")
        );
    }

    #[test]
    fn tool_box_system_section_is_none_when_no_tool_contributes_one() {
        let mut tools = ToolBox::new();
        tools
            .add(tool("ping", "ping", |_: WeatherArgs| async move { Ok("pong") }))
            .unwrap();
        assert!(tools.system_section().is_none());
    }

    #[test]
    fn tool_box_system_section_assembles_a_header_and_one_block_per_documented_tool() {
        let mut tools = ToolBox::new();
        tools
            .add(tool("ping", "ping", |_: WeatherArgs| async move { Ok("pong") }))
            .unwrap();
        tools
            .add(
                tool("bash", "run a shell command", |_: WeatherArgs| async move { Ok("ok") })
                    .with_system_prompt("Runs a command in a sandbox."),
            )
            .unwrap();

        let section = tools.system_section().expect("expected a tools section");
        assert!(section.starts_with("# Tools"));
        assert!(section.contains("## bash"));
        assert!(section.contains("Runs a command in a sandbox."));
        assert!(
            !section.contains("## ping"),
            "a tool with no system_prompt must not get its own subsection: {section}"
        );
    }
}
