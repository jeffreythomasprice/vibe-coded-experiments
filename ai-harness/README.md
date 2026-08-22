# ai-harness

Tauri v2 + Leptos desktop app. Workspace crates: `client` (Leptos frontend), `server` (Tauri backend), `lib` (server-side tools), `shared` (types shared across the IPC boundary).

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
