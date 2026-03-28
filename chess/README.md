# Chess

A multiplayer chess application with multiple game variants, AI opponents, and real-time play.

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Trunk](https://trunkrs.dev/) for building the frontend: `cargo install trunk`
- Docker (for PostgreSQL)

The `wasm32-unknown-unknown` target is installed automatically via `rust-toolchain.toml`.

## Setup

### Database

```sh
docker compose up -d
```

This starts PostgreSQL on port 5432. The server runs migrations automatically on startup.

### Secrets

Generate a JWT signing secret:

```sh
echo "JWT_SECRET=$(openssl rand -base64 32)" > server/.secrets
```

The server will refuse to start without this file.

## Running the server

```sh
cargo run -p chess-server
```

```sh
RUST_LOG=chess_server=trace,tower_http=debug cargo run -p chess-server
```

## Running the client

```sh
cd client
trunk serve
```

To build a static release bundle:

```sh
cd client
trunk build --release
```

Output goes to `client/dist/`.

## API

### POST /login

Authenticate with username and password. Returns a JWT token.

```sh
curl -X POST http://localhost:8001/login \
  -H 'Content-Type: application/json' \
  -d '{"username":"admin","password":"admin"}'
```

Use the returned token in subsequent requests:

```
Authorization: Bearer <token>
```

## Adding schemas

Drop a JSON Schema file into `shared/schemas/`. The build step automatically generates Rust types with serde derives. Use them from any crate via `chess_shared::YourTypeName`.
