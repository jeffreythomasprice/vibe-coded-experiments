# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Run

```bash
cargo build
cargo run          # renders a fullscreen sixel gradient, press any key to exit
```

## What This Is

A Rust terminal app that renders sixel graphics. It detects the terminal's pixel dimensions (falling back to cols×8 / rows×16), generates a full-screen rainbow gradient as RGBA, encodes it to sixel via `icy_sixel`, and writes it directly to stdout in raw mode. Logs go to `./logs/` via `tracing-appender`.

Key crates: `icy_sixel` (sixel encoding), `crossterm` (terminal control/raw mode), `tracing`/`tracing-subscriber`/`tracing-appender` (file logging), `anyhow` (error handling).

## Code Conventions

- Minimize comments. Only comment large blocks of opaque code (heavy math, bit shifts, dereferences, etc.).
- Prefer returning errors over `unwrap` or `expect`. Use `anyhow::Result` unless an obvious error type already exists.
