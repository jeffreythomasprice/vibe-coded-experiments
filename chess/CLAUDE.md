# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Multiplayer chess application with multiple game variants (standard, forced-capture-lose-all, forced-capture-checkmate), AI opponents, and real-time play. Rust workspace with three crates: `shared`, `server`, `client`.

## Commands

```sh
# Server
cargo run -p chess-server                    # starts on 0.0.0.0:8001
RUST_LOG=chess_server=trace,tower_http=debug cargo run -p chess-server

# Client (WASM via Trunk)
cd client && trunk serve                     # dev server on port 8000
cd client && trunk build --release           # static build to client/dist/

# Database
docker compose up -d                              # start PostgreSQL
docker compose down                               # stop PostgreSQL
docker compose down -v                             # stop and wipe data

# Build/check all crates
cargo build
cargo check --workspace
cargo clippy --workspace
```

## Architecture

### Schema-driven code generation

Domain types are defined as JSON Schema files in `shared/schemas/`. At build time, `shared/build.rs` uses **typify** to generate Rust structs with serde derives into `$OUT_DIR/codegen.rs`. The `shared/src/lib.rs` includes this via `include!()` and also exposes raw schema JSON strings via `chess_shared::schemas::*`.

To add a new domain type: drop a JSON Schema file into `shared/schemas/` with a `"title"` field. Cross-schema references via `$ref` are supported. The build step globs all `.json` files and resolves references automatically.

### Crate roles

- **chess-shared** — Generated types + raw schema strings. Both server and client depend on this. Runtime deps include `regress` (for JSON Schema pattern validation) and `uuid`.
- **chess-server** — Axum 0.8 + Tokio + sqlx (Postgres). Runs migrations on startup. `server/.env` must contain `DATABASE_URL`. Games are stored as JSONB.
- **chess-client** — Leptos 0.7 (CSR mode), compiled to WASM via Trunk. Currently a skeleton. Shares types with server through `chess-shared`.

### Request validation

Incoming request bodies are validated at runtime against their JSON Schema using `ValidatedJson<T>` (in `server/src/extractors.rs`). To add validation for a new request type, implement the `HasSchema` trait (one line mapping to the schema string constant). The extractor handles deserialization, schema validation, and error responses (400/422) automatically.

### Authentication

JWT-based auth via `AuthUser` extractor in `server/src/auth.rs`. Handlers that need authentication add `AuthUser` as a parameter — the extractor reads the `Authorization: Bearer <token>` header and validates the JWT. Login is via `POST /login`.

## Secrets

The server requires a JWT signing secret. Create `server/.secrets`:

```sh
echo "JWT_SECRET=$(openssl rand -base64 32)" > server/.secrets
```

This file is gitignored. The server will refuse to start without `JWT_SECRET`.

### Key constraints

- `wasm32-unknown-unknown` target is required (auto-installed via `rust-toolchain.toml`). Dependencies added to `shared` or `client` must be WASM-compatible.
- Release profile uses `opt-level = "z"` + LTO for minimal WASM size.
- Workspace dependencies (`serde`, `serde_json`, `tracing`) are declared in root `Cargo.toml` — use `{ workspace = true }` in member crates.
