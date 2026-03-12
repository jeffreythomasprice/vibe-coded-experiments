# FPS Counter

**Summary:** Track frame timing and display a live FPS counter in the overlay, replacing the existing "Hello, World!" text.
**Depends on:** texture-atlas-font, immediate-mode-2d-overlay

---

## Steps

### 1.1 Add frame-time tracking to `App`

**Files:** `src/main.rs`

Add fields to `App` to accumulate frame timing data and produce a smoothed FPS readout:

- `frame_count: u32` — frames since last FPS update.
- `fps_accum: f32` — accumulated dt since last FPS update.
- `fps_display: f32` — the FPS value currently shown (updated once per second).

In `App::new()`, initialize `frame_count: 0`, `fps_accum: 0.0`, `fps_display: 0.0`.

In `RedrawRequested`, after computing `dt`:
1. Increment `frame_count` and add `dt` to `fps_accum`.
2. When `fps_accum >= 1.0`, set `fps_display = frame_count as f32 / fps_accum`, then reset both to 0.

**Tests:** None (real-time loop; verified visually).

### 1.2 Replace "Hello, World!" with FPS display

**Files:** `src/main.rs`

In the `RedrawRequested` overlay section, replace the `font.draw_text("Hello, World!", ...)` call with:

```rust
let fps_text = format!("FPS: {:.0}", self.fps_display);
font.draw_text(&mut self.draw_list, &fps_text, 10.0, 10.0, Rgba::WHITE);
```

Position the text in the top-left corner (10, 10) so it doesn't overlap the scene center.

**Tests:** `cargo run` and verify the counter appears and updates roughly once per second.
