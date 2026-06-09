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

**Server** (binds `localhost:8001` by default; configure via `config.toml`):

```sh
cp server/config.example.toml ./config.toml   # optional; defaults work without it
cargo run -p server
```

Only one server instance may run at a time (enforced by an advisory lock at
`/tmp/roll20-clone/server.lock`); a second start exits with a non-zero code.

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

The server reads a TOML config file, searched for in this order (first wins):

1. a path passed as the first CLI argument (`cargo run -p server -- /path/config.toml`)
2. `./config.toml` (the working directory — workspace root under `cargo run`)
3. `~/.config/roll20-clone/config.toml`

If none is found, built-in defaults (identical to `server/config.example.toml`)
are used. The file sets `host`, `port`, the `log` directory, and the `db` file
path (a leading `~` is expanded). See `server/config.example.toml`.

The client is unchanged: `client/.env` (`SERVER_HTTP_URL` / `SERVER_WS_URL`) is
baked in at build time.

## Logging

Both crates use `tracing`: our code at `TRACE`, dependencies at `WARN`. The
server honors `RUST_LOG` and writes to **both** stderr and a rotating file under
the configured `log` directory (`server.log`, rotated at 10 MB or daily —
whichever comes first — keeping the 30 most recent). The client filter is fixed
at compile time (browser console output via `tracing-web`).
