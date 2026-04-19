# llm-rag

Rust CLI + daemon for a local LLM-with-RAG system. Early scaffolding — see `TODO.md` for the feature wishlist.

## Build and run

```bash
cargo build
cargo run -- <subcommand>      # e.g. ping, server
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt
```

The binary runs in two modes: `llm-rag server` is a long-lived daemon listening on a Unix socket; any other subcommand acts as a CLI client and auto-spawns a server if one isn't already running.

## Configuration

Two separate TOML files. The main `config.toml` is required; the `secrets.toml` companion is optional and only consulted when a feature needs a secret.

### `config.toml`

The main config holds non-sensitive runtime settings. The loader searches:

1. `./config.toml` (current working directory)
2. `$XDG_CONFIG_HOME/llm-rag/config.toml` (typically `~/.config/llm-rag/config.toml`)

Pass `--config <path>` to override. If the override is set, the file MUST exist.

Fields (all required):

```toml
# Seconds the server remains idle (no active connections) before self-terminating.
server_idle_timeout_secs = 10

# Seconds the client waits for a response from the server before giving up.
client_request_timeout_secs = 30

# Directory containing the llm-rag unix socket file.
socket_dir = "/tmp/llm-rag/sock"

# Directory where the tracing logger writes structured JSON logs.
log_dir = "/tmp/llm-rag/logs"
```

### `secrets.toml`

Secrets (API keys, tokens) live in a separate `secrets.toml` so they can be gitignored and given restrictive permissions independently of the main config. The loader searches:

1. `./secrets.toml` (current working directory)
2. `$XDG_CONFIG_HOME/llm-rag/secrets.toml` (typically `~/.config/llm-rag/secrets.toml`)

Pass `--secrets <path>` to override. **Unlike `--config`, the secrets file is optional** — if neither default location exists and no override is given, the binary runs with no secrets loaded. Features that require a secret (e.g. the planned Anthropic provider) error at use-site, not at startup.

To create one:

```bash
mkdir -p ~/.config/llm-rag
cat > ~/.config/llm-rag/secrets.toml <<'EOF'
# Anthropic API key for the cloud LLM provider.
# Get one at https://console.anthropic.com/settings/keys
anthropic_api_key = "sk-ant-api03-REPLACE-ME"
EOF

# Restrict permissions — the loader logs a warning if this file is
# group/world-accessible.
chmod 600 ~/.config/llm-rag/secrets.toml
```

All fields are optional; comment out or delete any you don't use. A starter template is checked in as `secrets.toml.example` — copy it next to your `config.toml` and edit.

Secrets are wrapped in `secrecy::SecretString`, so they print as `[REDACTED]` in any debug log and are zeroized on drop.

## Exit codes

Errors are reported as a one-line JSON blob on stderr plus a specific exit code:

| Code | Meaning |
|------|---------|
| 10 | Server already running |
| 11 | Connect timeout |
| 12 | Request timeout |
| 13 | Config or secrets file not found (when explicitly required) |
| 1  | Everything else |
