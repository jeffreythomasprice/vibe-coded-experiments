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

`AI_HARNESS_LIVE_ANTHROPIC_MODEL` / `AI_HARNESS_LIVE_OLLAMA_MODEL` /
`AI_HARNESS_LIVE_OPENAI_MODEL` / `AI_HARNESS_LIVE_OPENAI_IMAGE_MODEL` override
the model a live run uses, so a smoke test can point at something cheaper
(`claude-haiku-4-5`, `gpt-image-1-mini`) without touching the default config.

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
`ChatProvider` trait (`OpenAiClient` additionally implements `ImageProvider`,
since image generation is the one thing neither of the other two can do
here — Ollama's image-generation endpoint is macOS-only and Anthropic
doesn't offer one at all). There is no router: construct the client you want
and call it directly.

| | Anthropic | Ollama | OpenAI |
|---|---|---|---|
| Endpoint | `/v1/messages` | `/api/chat` (native) | `/v1/responses` + `/v1/images/generations` |
| Streaming | SSE | NDJSON | SSE |
| Images | not supported here | input only | generation only |
| Auth | `[llm.anthropic] api_key_env` | none | `[llm.openai] api_key_env` |

Every `api_key_env` key names an **environment variable**, not the key
itself — the effective config is dumped to the logs on every startup, so
storing a raw key in `config.toml` would leak it there. Set the named
variable (`ANTHROPIC_API_KEY` / `OPENAI_API_KEY` by default) in your shell,
or in `.env` (see [Environment variables](#environment-variables) above),
before running anything that talks to that provider.

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
