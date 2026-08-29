# ai-harness

Tauri v2 + Leptos desktop app. Workspace crates: `client` (Leptos frontend),
`server` (Tauri backend), `lib` (server-side tools), `shared` (types shared
across the IPC boundary). See `CLAUDE.md` for architecture notes.

## Setup

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
cargo install tauri-cli --locked
cp .env.example .env   # fill in API keys for the LLM providers you'll use
```

## Run (dev)

Hot-reloads: Trunk serves the frontend, Tauri rebuilds the backend on change.

```sh
pushd server && cargo tauri dev; popd
```

## Build

```sh
cargo build --workspace         # compile all crates
cd server && cargo tauri build  # release bundle, under target/release/bundle/
```

## Test

```sh
cargo test   # unit tests only — no network, no money
```

Live tests that call a real provider are opt-in (`AI_HARNESS_LIVE=1`) and
covered in `CLAUDE.md`, since `live_anthropic`/`live_openai` spend real money.
