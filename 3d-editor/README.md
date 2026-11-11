# 3d-editor

A brush scene editor driven by a GUI or an AI agent through one command API. See `docs/plan.md`.

```
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

Everything talks to one headless `editor-server` over a unix socket (`docs/server.md`). Clients start it on demand, so build the whole workspace first. The optional shared config is `~/.config/editor/config.toml` (or `$EDITOR_CONFIG`).

Edit a scene file from the command line:

```
cargo run -p editor-cli -- --file my.scene.toml new
cargo run -p editor-cli -- --file my.scene.toml run brush.create_box --args '{"min":[0,0,0],"max":[64,64,64]}'
cargo run -p editor-cli -- --file my.scene.toml apply batch.json
```

If the GUI or an agent already has the file open, the command edits that live session instead and does not save (add `--save`). Without `--file` it uses the server's focused session.

Feed JSON lines to one session, watch changes, inspect the server:

```
echo '{"op":"scene.summary"}' | cargo run -p editor-cli -- --file my.scene.toml session
cargo run -p editor-cli -- --file my.scene.toml watch
cargo run -p editor-cli -- sessions
cargo run -p editor-cli -- server stop
cargo run -p editor-cli -- ops
cargo run -p editor-cli -- schema brush.create_box
```

Serve the same commands to an AI agent over MCP (read-only unless flags allow more). With no file it joins whatever the GUI has open:

```
cargo build --workspace
claude mcp add editor -- $PWD/target/debug/editor-mcp --allow-mutate
```

Render a PNG (needs a Vulkan-capable adapter, no display):

```
cargo run -p editor-cli -- --file my.scene.toml screenshot --camera viewport.top --overlay grid,axes,labels -o top.png
```

Open the GUI (needs a display and a Vulkan adapter; Help > Controls lists the inputs). Agent and CLI edits appear in it live:

```
cargo run -p editor-app -- scenes/test.scene.toml
```
