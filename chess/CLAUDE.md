# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Multiplayer chess application with multiple game variants, AI opponents, and real-time play. Rust workspace with three crates: `shared`, `server`, `client`. Early stage — auth works end-to-end but game engine, WebSocket, AI, and most UI are not yet implemented.

### Game variants

- **standard** — Normal chess rules.
- **forced_capture_lose_all** — Must capture if possible; king is a normal piece; you win by losing all your pieces.
- **forced_capture_checkmate** — Must capture if possible; normal checkmate rules still apply.

## Commands

```sh
./dev.sh                                          # starts Docker, server (cargo watch), and Trunk in one command

# Or manually (run each in a separate terminal):
docker compose up -d                              # 1. start PostgreSQL
cargo run -p chess-server                         # 2. server on 0.0.0.0:8001
cd client && trunk serve                          # 3. client dev server on port 8000

RUST_LOG=chess_server=trace,tower_http=debug cargo run -p chess-server  # verbose server

cd client && trunk build --release           # static build to client/dist/

docker compose down                               # stop PostgreSQL
docker compose down -v                             # stop and wipe data

cargo build                                       # build all crates
cargo check --workspace
cargo clippy --workspace
```

## Architecture

### Schema-driven code generation

Domain types are defined as JSON Schema files in `shared/schemas/`. At build time, `shared/build.rs` uses **typify** to generate Rust structs with serde derives into `$OUT_DIR/codegen.rs`. The `shared/src/lib.rs` includes this via `include!()` and also exposes raw schema JSON strings via `chess_shared::schemas::*`.

To add a new domain type: drop a JSON Schema file into `shared/schemas/` with a `"title"` field. Cross-schema references via `$ref` are supported. The build step globs all `.json` files and resolves references automatically.

### Crate roles

- **chess-shared** — Generated types + raw schema strings. Both server and client depend on this. Runtime deps include `regress` (for JSON Schema pattern validation) and `uuid`.
- **chess-server** — Axum 0.8 + Tokio + sqlx (Postgres). Runs migrations on startup. `server/.env` must contain `DATABASE_URL`. Games are stored as JSONB. Endpoints: `POST /api/login`, full CRUD on `/api/users` (admin-gated create/update/delete, cursor-based pagination on list).
- **chess-client** — Leptos 0.7 (CSR mode), compiled to WASM via Trunk. Trunk proxies `/api/*` to `localhost:8001` (configured in `Trunk.toml`). Pages: login, home (skeleton), users (CRUD with infinite scroll). `client/src/api.rs` provides request helpers that auto-attach the Bearer token.

### Request validation

Incoming request bodies are validated at runtime against their JSON Schema using `ValidatedJson<T>` (in `server/src/extractors.rs`). To add validation for a new request type, implement the `HasSchema` trait (one line mapping to the schema string constant). The extractor handles deserialization, schema validation, and error responses (400/422) automatically.

### Authentication

JWT-based auth via `AuthUser` extractor in `server/src/auth.rs`. Handlers that need authentication add `AuthUser` as a parameter — the extractor reads the `Authorization: Bearer <token>` header and validates the JWT. Claims include `sub` (user ID), `username`, `is_admin`, with 24-hour expiry. Login is via `POST /api/login`.

Client-side: JWT is stored in localStorage (`auth_token` key). Expiry is checked by manually decoding the JWT payload (no library) in `client/src/auth.rs`. `AuthState` provides reactive signals (`authenticated`, `is_admin`) via Leptos context. `AuthGuard` component redirects unauthenticated users to `/login`. The `NavBar` conditionally shows admin-only links (e.g., Users) based on `is_admin`.

### Database

PostgreSQL via Docker. Migration `0001_initial.sql` creates `users` and `games` tables. Games store full state as a single JSONB column. Seed user: `admin` / `admin` (plaintext — hashing is a TODO). Migrations run automatically on server startup.

### Adding a new endpoint (end-to-end checklist)

1. Add JSON Schema file(s) in `shared/schemas/` with a `"title"` field → types auto-generate on next build.
2. Server route module in `server/src/routes/` — register in `server/src/routes/mod.rs` router.
3. If the endpoint takes a request body, add a `HasSchema` impl in `server/src/extractors.rs` and use `ValidatedJson<T>` as the handler parameter.
4. If the endpoint requires auth, add `AuthUser` as a handler parameter (extracts from `Authorization` header automatically).
5. Client: add API call in `client/src/api.rs`, page/component in `client/src/pages/`, wire into router in `client/src/main.rs`.

### Error response convention

All server errors return JSON: `{ "error": "snake_case_code" }` with optional `"message"` or `"details"` fields. Standard codes: `invalid_json` (400), `validation_failed` (422), `invalid_credentials` / `missing_token` / `invalid_token` (401), `admin_required` (403), `not_found` (404), `username_taken` (409), `internal_error` (500).

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
- No tests exist yet.
