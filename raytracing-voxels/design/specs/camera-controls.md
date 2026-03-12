# Camera Controls

**Summary:** Add a `Camera` struct using yaw/pitch angles (gimbal-lock-free for FPS-style cameras) with WASD keyboard movement and mouse-look, syncing to the existing `CameraUniforms` GPU buffer each frame.
**Depends on:** wgpu-raytracing-cube (all steps complete)

---

## Steps

### 2.1 Camera struct with yaw/pitch orientation

**Files:** `src/camera.rs`, `src/main.rs`

Create a new `Camera` struct that stores orientation as yaw/pitch angles and derives the basis vectors on demand:

- `struct Camera` with fields:
  - `position: [f32; 3]` — world-space position
  - `yaw: f32` — horizontal angle in radians (0 = looking along -Z, positive = turning left/counterclockwise from above)
  - `pitch: f32` — vertical angle in radians (0 = horizontal, positive = looking up), clamped to roughly `(-89deg, 89deg)` to prevent flipping
  - `fov: f32` — vertical FOV in radians
- Methods:
  - `fn new(position: [f32; 3], yaw: f32, pitch: f32, fov: f32) -> Self`
  - `fn forward(&self) -> [f32; 3]` — unit forward vector derived from yaw and pitch
  - `fn right(&self) -> [f32; 3]` — unit right vector (cross of forward with world up `[0,1,0]`)
  - `fn up(&self) -> [f32; 3]` — unit up vector (cross of right with forward)
  - `fn to_uniforms(&self, aspect: f32) -> CameraUniforms` — builds the GPU-side uniform struct
- Create a `Default` impl or `Camera::default()` that places the camera at `(2, 1.5, 3)` looking toward the origin (compute the appropriate yaw/pitch from the current hardcoded forward vector).
- Add `mod camera;` to `main.rs` and make `CameraUniforms` public (or move it to `camera.rs`).
- Replace the `camera: CameraUniforms` field in `Renderer` with the raw buffer only; have `App` own a `Camera` and pass uniforms into `Renderer::render()`.

### 2.2 Keyboard movement (WASD + Space/Shift)

**Files:** `src/main.rs`, `src/camera.rs`

Add methods to `Camera` for movement and wire up keyboard input:

- Add to `Camera`:
  - `fn move_forward(&mut self, distance: f32)` — translates position along the horizontal forward direction (yaw only, no pitch component, so movement stays on the XZ plane)
  - `fn move_right(&mut self, distance: f32)` — translates position along the right vector
  - `fn move_up(&mut self, distance: f32)` — translates position along world up `[0,1,0]`
- In `App`, track which movement keys are currently held using a small struct or bitflags (e.g., `InputState` with bools: `forward`, `back`, `left`, `right`, `up`, `down`).
- On `KeyboardInput` events, update the pressed/released state for:
  - `W` / `S` — forward / back
  - `A` / `D` — left / right
  - `Space` / `ShiftLeft` — up / down
- Each frame (in `RedrawRequested`), compute a delta-time (use `std::time::Instant`) and apply movement: for each held direction, call the appropriate `Camera` method with `speed * dt`.
- Use a movement speed constant (e.g., `const MOVE_SPEED: f32 = 3.0` units/sec).
- Pass `camera.to_uniforms(aspect)` to the renderer each frame.

### 2.3 Mouse look with cursor grab

**Files:** `src/main.rs`, `src/camera.rs`

Add mouse-driven camera rotation with pointer lock:

- Add to `Camera`:
  - `fn rotate(&mut self, delta_yaw: f32, delta_pitch: f32)` — adjusts yaw and pitch, clamping pitch to `(-89deg, 89deg)`.
- On window focus or a mouse click, grab the cursor:
  - `window.set_cursor_grab(CursorGrabMode::Locked)` (fall back to `Confined` if Locked isn't supported).
  - `window.set_cursor_visible(false)`.
- On `DeviceEvent::MouseMotion { delta: (dx, dy) }` (note: this is a device event, not a window event — need to handle `device_event` in `ApplicationHandler`):
  - Convert `dx`/`dy` to yaw/pitch deltas: `delta_yaw = -dx * sensitivity`, `delta_pitch = -dy * sensitivity`.
  - Call `camera.rotate(delta_yaw, delta_pitch)`.
  - Use a sensitivity constant (e.g., `const MOUSE_SENSITIVITY: f32 = 0.003` radians/pixel).
- On Escape, release the cursor grab and show the cursor (instead of immediately exiting). A second Escape (or when cursor is already ungrabbed) exits the app. Alternatively, clicking re-grabs.
- Mark the camera as dirty so uniforms are re-uploaded (or just always upload — it's cheap).

### 2.4 Delta-time and frame pacing

**Files:** `src/main.rs`

Ensure smooth movement independent of frame rate:

- Store a `last_frame: Instant` in `App`.
- At the start of each `RedrawRequested`, compute `dt = last_frame.elapsed().as_secs_f32()` and update `last_frame`.
- Cap `dt` to a maximum (e.g., `0.1` seconds) to prevent huge jumps after stalls or breakpoints.
- Pass `dt` to the movement logic from step 2.2.
- Call `window.request_redraw()` at the end of `RedrawRequested` to drive continuous rendering (already done in existing code).
