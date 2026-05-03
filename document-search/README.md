# document-search

A scaffold for a local document-search tool: turso (SQLite/libSQL) for storage,
Ollama for embeddings, native vector search via `vector_distance_cos` on
`F32_BLOB(N)` columns.

## Build

```sh
cargo build --release
```

## Configure

The binary looks for `config.toml` in this order, taking the first one that
exists:

1. The path passed via `--config <PATH>` (errors if missing)
2. `./config.toml` in the current working directory
3. `~/.config/document-search/config.toml` (XDG `$XDG_CONFIG_HOME`)

Copy the bundled example into place:

```sh
mkdir -p ~/.config/document-search
cp config.example.toml ~/.config/document-search/config.toml
```

Edit it to point at your DB file and Ollama instance. Relative `db_path`
values are resolved against the config file's directory.

## Prerequisites

[Ollama](https://ollama.com) running locally, with an embedding model pulled:

```sh
ollama pull nomic-embed-text
```

The first time the binary runs it probes the configured model for its vector
length and caches the result in the DB; subsequent runs skip the probe.

## Run

```sh
cargo run                          # uses ~/.config/document-search/config.toml
cargo run -- --config ./other.toml # explicit override
```

## Logging

Tracing defaults to `warn` for everything and `trace` for this crate. Override
via `RUST_LOG`:

```sh
RUST_LOG=document_search=debug cargo run
RUST_LOG=trace cargo run
```

## Reset

Kill any running server, remove its socket, and wipe the DB:

```sh
killall -9 document-search 2>/dev/null
rm -f /tmp/document-search.sock
rm -f document-search.db document-search.db-wal document-search.db-shm
```

The `rm` paths assume the default `db_path` from `config.example.toml`; adjust
them if you've configured a different `db_path`.
