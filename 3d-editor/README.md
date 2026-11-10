# 3d-editor

A brush level editor driven by a GUI or an AI agent through one command API. See `docs/plan.md`.

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

Edit a level file from the command line:

```
cargo run -p level-cli -- my.level.toml new
cargo run -p level-cli -- my.level.toml run brush.create_box --args '{"min":[0,0,0],"max":[64,64,64]}'
cargo run -p level-cli -- my.level.toml apply batch.json
```

Keep one in-memory session (undo, selection, transactions) and feed it JSON lines:

```
echo '{"op":"scene.summary"}' | cargo run -p level-cli -- my.level.toml session
cargo run -p level-cli -- ops
cargo run -p level-cli -- schema brush.create_box
```

Serve the same commands to an AI agent over MCP (read-only unless flags allow more):

```
cargo build -p level-mcp
claude mcp add editor -- $PWD/target/debug/level-mcp my.level.toml --create --allow-mutate
```

Render a PNG (needs a Vulkan-capable adapter, no display):

```
cargo run -p level-cli -- my.level.toml screenshot --camera viewport.top --overlay grid,axes,labels -o top.png
```
