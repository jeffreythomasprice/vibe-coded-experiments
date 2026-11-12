# The game client

`editor-game` shows a scene as it is played: one first-person camera, no grid, no editor widgets, no viewports other than the one window. It is a client of `editor-server` like the GUI, so it always shows the live document, and it animates entities with the simulation described in [ecs.md](ecs.md).

## Running

```
cargo build --workspace            # editor-server must sit next to the client
cargo run -p editor-game [scene.toml | --session <id>]
```

- A path attaches to (or opens) the session for that scene file.
- `--session <id>` attaches to an open session; the editor's Play button (F5) uses this.
- No argument joins the server's focused session, or opens `scenes/demo-courtyard.scene.toml` when there is none.
- It connects as client `game` with no mutate access, so it never edits the document and never steals the editor's focus.

`scenes/demo-courtyard.scene.toml` has two door leaves (`e33`, `e61`) that swing open and shut every 7 seconds.

## Controls

| Input | Action |
|---|---|
| Left click | capture the mouse (it is hidden and confined to the window) |
| Mouse | look |
| W A S D or arrow keys | move along the view (forward moves where you look, including up and down) |
| Space / E | up |
| Left Ctrl / Q | down |
| Left or right Shift | 3x speed |
| P | pause or resume the simulation |
| R | restart the simulation at time 0 |
| Esc | release the mouse; Esc again quits |

The window title shows the scene name, the simulation time and `[paused]`.

Movement is noclip fly at 320 units per second. There is no gravity, no jumping and no collision yet.

## Behaviour

- **Camera:** vertical field of view 75 degrees. It starts at the `player_start` entity (see [components.md](components.md#player_start)), else at the framed `viewport.persp` camera.
- **Live updates:** every change event from the server rebuilds the simulation and the draw lists; the elapsed time is kept, so a running animation continues. Editing a keyframe in the editor is visible in the game immediately.
- **What is drawn:** brushes and meshes with their materials and baked directional shading. No entity markers, no edge lines, no selection. Editor `hidden` flags are ignored; layers marked `omit_from_export` are not drawn.
- **Movers:** each entity's brushes and meshes are transformed by its animation pose when drawn; static geometry is drawn as is.
- **Frame loop:** a winit window with a polling loop and vsync. The scene is drawn by `editor-render` into an offscreen `Rgba8Unorm` image and blitted to the surface.

## Seeing it without a window

`view.screenshot` takes `simulate: {time}` and draws the scene exactly as the game client would at that time: movers posed, no markers or edges, hidden flags ignored.

```
editor-cli screenshot --args '{"camera": "gate", "simulate": {"time": 2.5}}' -o gate.png
```

## Known limits

- Lighting is baked into vertex colours when the scene is built, so a rotating mover keeps its rest shading.
- The renderer rebuilds its GPU buffers every frame (see [gui.md](gui.md)); fine for demo scenes, a cost for large ones.
- No visibility culling: every face is drawn every frame.
- No collision, physics, sound or lights; `light`, `sound_source` and the logic components are data only for now.
