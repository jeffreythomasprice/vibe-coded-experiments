# Setup

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk
```

# Run

```bash
trunk serve
```

Then open http://localhost:8000. Use `trunk build --release` for a production build in `dist/`.
