TODO.md is for humans, don't update it, and try to avoid even referencing it unless specifically propmted

Prefer unit tests to prove a change works whenever possible.

When you do need to run the app to prove something, use short timeouts. The app
starts up and loads pretty much instantly, so a few seconds is plenty — don't
waste minutes on things like `timeout 240`.

For `lib::llm`, "prefer unit tests" means something specific: request
building, response/error parsing, and stream framing are all pure functions
over strings, tested against recorded fixtures in each provider's
`fixtures/` directory — no network involved. If you touch a provider's wire
format, add or update a fixture and a unit test rather than reaching for a
live call. The live tests in `lib/tests/` (`AI_HARNESS_LIVE=1 cargo test -p
lib --test live_<provider>`) actually call Anthropic, OpenAI, or a local
Ollama; Anthropic and OpenAI spend real money, so don't run them just to
check something works — run them when you need to confirm end-to-end
behavior against the real API, not as a substitute for a unit test.

## Live tests

`live_anthropic` and `live_openai` spend real money; `live_ollama` is free
but needs a local `ollama serve` with the right models pulled — each test
also skips (rather than fails) if its own prerequisite (an API key, a
reachable Ollama) is missing, so a misconfigured run reads as "skipped," not
as a broken build.

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

## UI / end-to-end tests

`server/tests/webdriver_ui.rs` drives the actual compiled `server` binary
through a real webview via [`tauri-driver`](https://github.com/tauri-apps/tauri-driver)
— Tauri's answer to "what does Cypress look like for a native app," since
Cypress itself can't drive an arbitrary WebKitGTK/WRY window. On Linux,
`tauri-driver` shells out to `WebKitWebDriver` (from the `webkit2gtk-driver`
apt package) and speaks the standard WebDriver protocol, so a Rust client
(`fantoccini`) can click real elements and pull back real PNG screenshots —
no OS-level screenshot tool or window-manager cooperation required, which is
what made every prior ad hoc "run it and screenshot the window" attempt
unreliable. This is also the layer no unit test can reach: it's the only
place that exercises real Tauri IPC dispatch, a real (temp, isolated)
SQLite db through `lib::service`, and the real Leptos wasm bundle in one
pass, the way a user's click actually does.

One-time setup (Linux):

```sh
sudo apt-get install -y webkit2gtk-driver   # provides WebKitWebDriver
cargo install tauri-driver
```

To run:

```sh
Xvfb :99 -screen 0 1280x800x24 &            # keeps the real webview off your actual desktop
DISPLAY=:99 tauri-driver &
(cd client && trunk serve &)                # see the gotcha below — needed regardless of profile
AI_HARNESS_E2E=1 cargo test -p server --test webdriver_ui -- --nocapture --test-threads=1
```

Like the live tests, a plain `cargo test` compiles this file but every test
returns immediately unless `AI_HARNESS_E2E=1` is set, and each one also
skips (rather than fails) if `tauri-driver` isn't reachable or the app never
renders its shell (most likely cause: `trunk serve` isn't running) — so a
misconfigured run reads as "skipped," not as a broken build. Use
`--test-threads=1`: each test spawns its own real app process and webview
window, and there's no reason to make that race.

Gotcha: the compiled `server` binary always points its window at
`tauri.conf.json`'s `devUrl` (`http://localhost:1420`) — true for `cargo
build` **and** `cargo build --release` alike. Only a full `cargo tauri
build` (the CLI's own bundling pipeline) embeds `frontendDist` instead. So
`trunk serve` has to be running for these tests no matter the profile —
exactly what `cargo tauri dev` already arranges for you.

Each test gets its own throwaway `--config` (temp dir: db, cache, logs,
sandbox disabled) via `server/tests/common::launch`, so these never touch a
real `~/.config/ai-harness` database. `App::screenshot(name)` saves a PNG to
`target/e2e-screenshots/<name>.png` (gitignored) — reach for it from a
one-off `#[tokio::test]` for ad hoc visual inspection of a view, not only
from a committed assertion.

## Environment variables

Copy `.env.example` to `.env` in the repo root and fill in the keys for
whichever LLM providers you plan to use. `.env` is loaded automatically on
startup (and by the live LLM tests) and is already listed in `.gitignore`,
so it's never committed. A variable already set in your shell takes
precedence over the file. See [LLM providers](#llm-providers) below for what
each key is used for.

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

## LLM providers

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

## Agents

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

## Projects

`shared::project` defines a **project**: a name plus zero or more real
directories, each read-only or read-write, that becomes an agent's sandbox.
Directories are never remapped — `lib::vfs::MountTable` is a *filter* over the
real filesystem, so a project's configured directories appear at their real
absolute paths and nothing else exists (a synthetic ancestor directory lists
only the path down to a mounted one). The **default project** (zero
directories, no filesystem access at all) is never a database row — it's
synthesized, the same way `shared::theme::BuiltIn` themes are compiled in and
never rows in `themes`.

`lib::vfs` is the pure, unit-tested resolver (`MountTable::resolve`) plus the
in-process file primitives (`Vfs`), using `openat2(RESOLVE_IN_ROOT |
RESOLVE_NO_MAGICLINKS)` for symlink-safe, race-free containment. `lib::sandbox`
builds the confined `bwrap` (bubblewrap) command a subprocess runs under,
layering a read-only system image (`/usr`, `/bin`, minimal `/etc` —
`[sandbox] system_paths`) under the project's own mounts so a shell has an OS
to run in. Both consume the same `MountTable`, so an in-process file tool and
a sandboxed shell can never disagree about what exists. See `lib::sandbox`'s
module doc for the empirical basis (checked against a real unprivileged
`bwrap` run) and `lib/tests/sandbox_bwrap.rs` for the end-to-end proof — that
test isn't gated behind `AI_HARNESS_LIVE` since `bwrap` is a free local binary.

**Agent configs are frozen into a conversation; projects are not — this is
deliberate, not an inconsistency.** `conversations.agent_config_json` copies
the whole `AgentConfig` verbatim at creation time, because a system prompt is
a *premise*: editing the saved agent later must not rewrite the premise of
history that already ran (see `sql/0001_init.sql`). `conversations.project_id`
is a soft link with no frozen counterpart — `Service::conversation_mounts`
re-resolves it from the `projects` table on every use. A project is a *live
grant*, not a premise: narrowing it (removing a directory, or deleting the
project outright) must take effect immediately for every conversation open on
it, not just new ones, or the sandbox boundary isn't actually a boundary.
`project_id: NULL` means "the default project," covering both "never assigned
one" and "assigned project since deleted" — both resolve to the same empty,
safe virtual filesystem, so that ambiguity is harmless by construction.

## Persistence

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
