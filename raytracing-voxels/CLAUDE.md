# Raytracing Voxels - Style Guide

## Error handling

- Never use `.unwrap()` or `.expect()`. Propagate errors with `?` instead.
- Use `anyhow::Result` as the default error type unless a more specific error type already exists.
- For functions that can't meaningfully fail, don't wrap in Result — just return the value directly.
- In `main()`, return `anyhow::Result<()>` and use `?` for all fallible operations.

## Dependencies

- `anyhow` for error handling.

## Commands

- Build: `cargo build`
- Test: `cargo test`
- Lint: `cargo clippy`
