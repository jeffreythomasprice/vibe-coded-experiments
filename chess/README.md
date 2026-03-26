# Chess

A multiplayer chess application with multiple game variants, AI opponents, and real-time play.

## Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Trunk](https://trunkrs.dev/) for building the frontend: `cargo install trunk`

The `wasm32-unknown-unknown` target is installed automatically via `rust-toolchain.toml`.

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

## Adding schemas

Drop a JSON Schema file into `shared/schemas/`. The build step automatically generates Rust types with serde derives. Use them from any crate via `chess_shared::YourTypeName`.
