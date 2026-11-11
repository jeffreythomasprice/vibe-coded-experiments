# 3d-editor

A brush level editor driven by a GUI or an AI agent through one command API. See `docs/plan.md`.

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

Everything talks to one headless `level-server` over a unix socket (`docs/server.md`). Clients start it on demand, so build the whole workspace first. The optional shared config is `~/.config/level-editor/config.toml` (or `$LEVEL_EDITOR_CONFIG`).

Edit a level file from the command line:

```
cargo run -p level-cli -- --file my.level.toml new
cargo run -p level-cli -- --file my.level.toml run brush.create_box --args '{"min":[0,0,0],"max":[64,64,64]}'
cargo run -p level-cli -- --file my.level.toml apply batch.json
```

If the GUI or an agent already has the file open, the command edits that live session instead and does not save (add `--save`). Without `--file` it uses the server's focused session.

Feed JSON lines to one session, watch changes, inspect the server:

```
echo '{"op":"scene.summary"}' | cargo run -p level-cli -- --file my.level.toml session
cargo run -p level-cli -- --file my.level.toml watch
cargo run -p level-cli -- sessions
cargo run -p level-cli -- server stop
cargo run -p level-cli -- ops
cargo run -p level-cli -- schema brush.create_box
```

Serve the same commands to an AI agent over MCP (read-only unless flags allow more). With no file it joins whatever the GUI has open:

```
cargo build --workspace
claude mcp add editor -- $PWD/target/debug/level-mcp --allow-mutate
```

Render a PNG (needs a Vulkan-capable adapter, no display):

```
cargo run -p level-cli -- --file my.level.toml screenshot --camera viewport.top --overlay grid,axes,labels -o top.png
```

Open the GUI (needs a display and a Vulkan adapter; Help > Controls lists the inputs). Agent and CLI edits appear in it live:

```
cargo run -p editor-app -- scenes/test-scene.level.toml
```
