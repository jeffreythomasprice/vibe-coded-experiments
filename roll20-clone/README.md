# roll20-clone

A multi-project Rust workspace for a Roll20-style virtual tabletop.

## Crates

| Crate     | What it is                                                          |
| --------- | ------------------------------------------------------------------ |
| `shared`  | Serde data types for HTTP and WebSocket messages (used by both).   |
| `server`  | Axum HTTP + WebSocket server. Real-time updates via a broadcast hub.|
| `client`  | Leptos (CSR) web UI, served by Trunk, connecting to the server.    |

## Prerequisites

```sh
rustup target add wasm32-unknown-unknown
cargo install trunk
```

## Running

**Server** (binds `localhost:8001` by default; configure via `server/.env`):

```sh
cargo run -p server
```

- `GET /health` → `{"status":"ok"}`
- `GET /api/version`
- `GET /ws` → WebSocket; messages from any client are broadcast to all.

**Client** (Trunk dev server on `localhost:8000`):

```sh
cd client && trunk serve
```

The client's view of the server is baked in at build time from `client/.env`
(wasm has no runtime env). Edit `client/.env` and rebuild to point elsewhere.

## Configuration

- `server/.env` — `SERVER_HOST` / `SERVER_PORT` (defaults `127.0.0.1:8001`).
- `client/.env` — `SERVER_HTTP_URL` / `SERVER_WS_URL` (defaults to localhost:8001).

## Logging

Both crates use `tracing`: our code at `TRACE`, dependencies at `WARN`. The
server honors `RUST_LOG`; the client filter is fixed at compile time (browser
console output via `tracing-web`).
