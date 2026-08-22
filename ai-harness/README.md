# ai-harness

Tauri v2 + Leptos desktop app. Workspace crates: `client` (Leptos frontend), `server` (Tauri backend), `lib` (server-side tools), `shared` (types shared across the IPC boundary).

## Environment variables

Copy [`.env.example`](.env.example) to `.env` in the repo root and fill in
the keys for whichever LLM providers you plan to use:

```sh
cp .env.example .env
```

`.env` is loaded automatically on startup (and by the live LLM tests) and is
already listed in `.gitignore`, so it's never committed. A variable already
set in your shell takes precedence over the file. See
[LLM providers](#llm-providers) below for what each key is used for.

## Prerequisites

One-time setup — Rust's wasm target, Trunk (builds the Leptos frontend), and the Tauri CLI.

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cargo install tauri-cli --locked
```

## Build

Compiles all four crates.

```sh
cargo build --workspace
```

## Run (dev)

Launches the app with hot-reload (Trunk serves the frontend, Tauri rebuilds the backend on change). Run from `server/`, where `tauri.conf.json` lives.

```sh
cd server && cargo tauri dev
```

## Build a release bundle

Produces an installable package under `target/release/bundle/`.

```sh
cd server && cargo tauri build
```

## Tests

Unit tests never touch the network and are the default — they cover request
building, response/error parsing, and stream framing against recorded
fixtures, all of it pure functions over strings with no live API involved.

```sh
cargo test   # unit tests only — no network, no money
```

Integration tests that actually call a provider live in `lib/tests/` and are
opt-in: a plain `cargo test` compiles them, but every test inside returns
immediately unless `AI_HARNESS_LIVE=1` is set. `live_anthropic` and
`live_openai` spend real money; `live_ollama` is free but needs a local
`ollama serve` with the right models pulled — each test also skips (rather
than fails) if its own prerequisite (an API key, a reachable Ollama) is
missing, so a misconfigured run reads as "skipped," not as a broken build.

```sh
AI_HARNESS_LIVE=1 cargo test -p lib --test live_ollama -- --nocapture     # local, free
AI_HARNESS_LIVE=1 cargo test -p lib --test live_anthropic -- --nocapture  # real money
AI_HARNESS_LIVE=1 cargo test -p lib --test live_openai -- --nocapture     # real money
```

The model each live test calls is a `const` at the top of its own
`lib/tests/live_*.rs` file, not a config default (see
[Configuration](#configuration) below for why) — point a smoke run at
something cheaper (`claude-haiku-4-5`, `gpt-image-1-mini`) by editing that
constant.

## Configuration

TOML, read from the first location that exists:

1. `--config <path>` — a path given here that doesn't exist is a hard error
2. `./config.toml` — the working directory
3. `~/.config/ai-harness/config.toml`
4. built-in defaults

See [`config.example.toml`](config.example.toml) for every key and its default;
copy it to `~/.config/ai-harness/config.toml` to start from it. Unknown keys are
rejected rather than ignored, so typos fail loudly. The effective config and
where it was loaded from are logged on startup.

```sh
cargo run -p server -- --config ./config.example.toml
# Through the Tauri CLI, app args go after a second `--`:
cd server && cargo tauri dev -- -- --config ../config.example.toml
```

### LLM providers

`lib::llm` is a hand-rolled client for three providers, each behind the same
`ChatProvider` trait. `OpenAiClient` additionally implements `ImageProvider`,
since image generation is the one thing neither of the other two can do
here — Ollama's image-generation endpoint is macOS-only and Anthropic
doesn't offer one at all. `OpenAiClient` and `OllamaClient` also implement
`EmbeddingProvider`; `AnthropicClient` does not — Anthropic's API surface is
Messages, Batches, Files, Token Counting, and Models, with no embeddings
endpoint. A caller that wants one specific provider still constructs that
client directly and calls it; a caller that wants to address "whichever
provider a `shared::llm::ModelRef` names" goes through
`lib::llm::router::Router` instead — see [Agents](#agents) below, which is
built on it.

| | Anthropic | Ollama | OpenAI |
|---|---|---|---|
| Endpoint | `/v1/messages` | `/api/chat` (native) | `/v1/responses` + `/v1/images/generations` |
| Streaming | SSE | NDJSON | SSE |
| Images | not supported here | input only | generation only |
| Embeddings | not supported here | `/api/embed` | `/v1/embeddings` |
| Auth | `[llm.anthropic] api_key_env` | none | `[llm.openai] api_key_env` |

Every `api_key_env` key names an **environment variable**, not the key
itself — the effective config is dumped to the logs on every startup, so
storing a raw key in `config.toml` would leak it there. Set the named
variable (`ANTHROPIC_API_KEY` / `OPENAI_API_KEY` by default) in your shell,
or in `.env` (see [Environment variables](#environment-variables) above),
before running anything that talks to that provider.

Deliberately absent from every `[llm.*]` table: a model id, image model,
embedding model, or `max_tokens`. Those are per-request decisions, not
deployment settings, so they're required fields on `ChatOptions` /
`EmbeddingRequest` / `ImageRequest` (`shared::llm`) instead of config
defaults — a caller that forgets to pick a model gets a compile error, not a
request silently sent to whatever id was baked in months ago.

### Agents

`lib::agent::Agent` is a model, a system prompt, and a set of tools, plus the
loop that drives a conversation through them. `shared::agent::AgentSpec` is
the serializable definition (crosses the Tauri IPC boundary the same way
`shared::llm` does); `lib::agent` is the executable side — the `Tool` trait,
the loop, and the router-backed builder.

```rust
let router = Router::from_config(&config.llm);

let weather = tool("get_weather", "Look up the weather", |args: WeatherArgs| async move {
    Ok(format!("72F and sunny in {}", args.city))
});
let deploy = tool("deploy", "Ship to prod", |args: DeployArgs| async move { .. })
    .requiring_approval();

let agent = Agent::builder(ModelRef::new("anthropic", "claude-opus-5"), 1024)
    .system("You are a helpful ops assistant.")
    .tool(weather)
    .tool(deploy)
    .build(&router)?;

let turn = agent.next_turn(conversation).await?;
```

Three ways to write a tool, in `lib::agent::tool`: implement the `Tool` trait
directly for full control; `tool(name, description, handler)` for a plain
Rust fn whose argument type derives `JsonSchema` (re-exported from
`schemars`) — the ergonomic default; or `json_tool(name, description,
schema, handler)` for a hand-written schema when there's no Rust type to
derive one from (a tool assembled from config or a remote listing).

`Agent::next_turn`/`send` run the full tool loop — calling the model,
executing every automatic tool call, feeding the results back, and repeating
— except for a tool marked `.requiring_approval()`. When the model asks for
one of those, the turn suspends at `TurnStop::AwaitingApproval` (any
automatic calls from the same step already ran; their results ride in
`completed` rather than being sent, since a provider requires a
`tool_result` for every `tool_use` in the preceding message — a partial set
isn't a sendable request on its own) and `Agent::resume` continues it once
every pending call has an `Approve` or `Deny` decision. `Agent::stream_turn`
/ `resume_stream` are the streaming equivalents, emitting a `StepStart` /
`Model` / `ToolStart` / `ToolEnd` event stream that terminates in the same
`AgentTurn` the blocking call would have returned.

`max_tokens` on `AgentSpec` applies **per model call**, not per turn —
`max_steps` (default 8) is the control for a multi-step turn's total
cost/runaway risk.

### Persistence

Agent configs and conversations are stored in one SQLite file, at the path
named by `[database]` in `config.toml` (default `/tmp/ai-harness/ai-harness.db`
— see [`config.example.toml`](config.example.toml)). It's a plain SQLite
file — nothing Turso/libSQL-specific — so `sqlx`'s driver can be swapped for
`sqlx-turso` later without touching anything above `lib::db`.

- `lib::db::migrate` — a hand-rolled runner, not `sqlx::migrate!`: a
  migration is `Step::Sql(&str)` or `Step::Rust(fn)`, so a backfill that has
  to read a row, transform it in Rust, and write it back is possible, not
  just schema DDL. Each migration and its `_migrations` ledger row commit in
  one transaction. `Db::open` runs this on startup.
- `lib::db` (`agents`, `conversations`, `turns`) — the DAL. A turn's messages
  are held back from `messages` until it reaches a terminal outcome, so the
  table never holds an unresolved `tool_use`; a turn suspended on
  `TurnStop::AwaitingApproval` is stored as the whole `AgentTurn`, serialized
  verbatim, so `Agent::resume` can't see a reconstructed value that doesn't
  match what it suspended on. See that module's doc for the rest of the
  invariants.
- `lib::agent::registry::ToolRegistry` — the process-wide catalog a stored
  `AgentConfig`'s tool selection resolves against; `build_agent` turns a
  config into a runnable `Agent`.
- `lib::service::Service` — the layer Tauri commands call into: CRUD on
  agent configs, and `send_message`/`approve_tools`, which drive a turn and
  persist it only at its terminal outcome — never partway through a stream
  (a truncated assistant message isn't replayable). One lock per
  conversation keeps two turns from racing on the same history.

## Logs

Every crate logs through `tracing`. Lines carry an ISO8601 UTC timestamp with
millisecond precision, the level, the module path, and the source location:

```
2026-08-22T00:47:03.955Z  INFO server: server/src/main.rs:54: starting tauri
```

Logs go to stderr and to a rotating file in `log.dir` (default
`/tmp/ai-harness/logs`). The active file is always `ai-harness.log` — a stable
path, so `tail -f` survives rotations — and rotated files are renamed
`ai-harness-<rotation timestamp>.log`. Rotation happens on the configured
interval or size cap, whichever comes first, keeping the last `keep` files.

By default our crates log at TRACE and all dependencies at WARN. `RUST_LOG`
overrides the config file's `filter`:

```sh
RUST_LOG=trace cd server && cargo tauri dev   # everything, including tauri/wry
```

Frontend logs appear in the browser devtools console *and* are forwarded over
IPC into the same files under the `client` target. Because `tracing` bakes its
metadata in statically, a forwarded line's `file:line` column points at the
forwarding bridge — the real origin is in the `client.file` / `client.line`
fields, and `client.time` is the browser-side timestamp:

```
2026-08-22T00:47:04.303Z  INFO client: lib/src/logging/mod.rs:107: client starting \
  client.time=2026-08-22T00:47:04.299Z client.target=client \
  client.file=client/src/main.rs client.line=18
```
