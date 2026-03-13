# Tile Picker UI

**Summary:** Replace the current bottom-left voxel selector with a centered tile picker bar showing all tile types as textured squares with keyboard labels, highlight for the selected tile, and key-0 to deselect (remove-only mode).
**Depends on:** voxel-editing, textured-voxels, immediate-mode-2d-overlay

---

## Steps

### 1. Extend voxel type constants and key mapping

**Files:** `src/main.rs`

Update `VOXEL_TYPE_NAMES` and `VOXEL_KEY_TO_ID` to cover all 6 tile types (stone=1, dirt=2, grass=3, brick=4, wood=5, leaves=6) plus a key-0 entry for "no selection":

```rust
const VOXEL_TYPE_NAMES: &[&str] = &["", "stone", "dirt", "grass", "brick", "wood", "leaves"];
const VOXEL_KEY_TO_ID: [u8; 7] = [0, 3, 2, 1, 4, 5, 6];
```

Add keyboard handlers for `Digit5`, `Digit6`, and `Digit0`. When `Digit0` is pressed, set `active_voxel_type = 0`.

Change `active_voxel_type` from `u8` to `Option<u8>` — `None` means no tile selected (remove-only mode), `Some(id)` means a tile is selected. Update the `Digit0` handler to set it to `None`, and `Digit1`–`Digit6` to set it to `Some(VOXEL_KEY_TO_ID[n])`.

### 2. Update placement logic for remove-only mode

**Files:** `src/main.rs`

Modify the left-click handler: when `active_voxel_type` is `None`, left-click should do nothing (no placement). Only place a voxel when `active_voxel_type` is `Some(id)`. Right-click removal remains unchanged regardless of selection state.

Update the `"Active: {}"` text display and the voxel preview HUD to handle `None` — show "Active: none" or skip the preview quad when no tile is selected.

### 3. Draw the tile picker background and border

**Files:** `src/main.rs`

Replace the old bottom-left voxel preview HUD with a centered tile picker bar. Use the font overlay draw list to draw the background elements (since `solid_rect` doesn't need a specific texture — the font atlas will work with UV 0,0 to 0,0 or a white pixel region).

Layout constants:
```rust
const TILE_PICKER_TILE_SIZE: f32 = 48.0;
const TILE_PICKER_SPACING: f32 = 8.0;
const TILE_PICKER_PADDING: f32 = 12.0;
const TILE_PICKER_BORDER: f32 = 3.0;
const TILE_PICKER_BG_COLOR: Rgba = Rgba::new(30, 30, 30, 200);
const TILE_PICKER_BORDER_COLOR: Rgba = Rgba::new(80, 80, 80, 220);
const TILE_PICKER_HIGHLIGHT_COLOR: Rgba = Rgba::new(255, 255, 100, 180);
```

Compute the total width of the picker: `num_tiles * TILE_SIZE + (num_tiles - 1) * SPACING + 2 * PADDING`. Center it horizontally: `x_start = (screen_width - total_width) / 2`. Position it near the bottom of the screen with some margin.

Draw two rectangles into the font draw list:
1. Outer border rect (using `TILE_PICKER_BORDER_COLOR`)
2. Inner background rect inset by `TILE_PICKER_BORDER` (using `TILE_PICKER_BG_COLOR`)

These are drawn as `solid_rect` calls which use the font texture but with a solid color appearance (the font atlas already has the UV mapping such that a white region exists, or we can draw with UV 0,0→0,0 which samples a corner pixel — if this doesn't produce a clean solid color, add a 1x1 white pixel texture or use a dedicated solid-color draw list with a tiny white texture).

Actually, `solid_rect` uses `Vec2::ZERO` to `Vec2::ONE` for UV, which will sample the entire font atlas — not correct for a solid color. We need to either:
- Add a `filled_rect` method to `DrawList` that draws a solid-colored quad by sampling a known-white texel, or
- Create a 1x1 white `Texture` and bind group for solid-color overlay rendering, with its own draw list.

The simplest approach: create a 1x1 white texture + bind group (`solid_bind_group`) and a `picker_bg_draw_list: DrawList` for all solid-color quads (background, border, highlight). Render this as an additional overlay pass.

### 4. Draw tile texture previews in the picker

**Files:** `src/main.rs`

For each tile type (IDs 1–6), draw a `TILE_PICKER_TILE_SIZE × TILE_PICKER_TILE_SIZE` quad into `hud_draw_list` using the voxel atlas UV rect from `self.voxel_uv_map[tile_id]`. Position each tile within the picker grid:

```rust
let tile_x = x_start + TILE_PICKER_PADDING + i * (TILE_PICKER_TILE_SIZE + TILE_PICKER_SPACING);
let tile_y = picker_y + TILE_PICKER_PADDING;
```

This reuses the existing `voxel_atlas_overlay_bind_group` and `hud_draw_list`. Remove the old single-preview quad code.

### 5. Draw highlight around the selected tile

**Files:** `src/main.rs`

When `active_voxel_type` is `Some(id)`, determine which tile index corresponds to that ID and draw a highlight rect behind that tile's position (slightly larger than the tile, e.g. inset by -3px on each side) using `TILE_PICKER_HIGHLIGHT_COLOR` into the solid-color draw list. This highlight is drawn before the tile textures so it appears behind them as a border/glow.

When `active_voxel_type` is `None`, no highlight is drawn.

### 6. Draw key labels below each tile

**Files:** `src/main.rs`

Use the existing font to draw the key number ("1", "2", ..., "6") centered below each tile in the picker. Position the text at:

```rust
let label_x = tile_x + TILE_PICKER_TILE_SIZE / 2.0 - char_width / 2.0;
let label_y = tile_y + TILE_PICKER_TILE_SIZE + 4.0;
```

Use `Rgba::WHITE` for unselected tiles, and `TILE_PICKER_HIGHLIGHT_COLOR` (or a brighter variant) for the selected tile's label.

Also add a small "0: none" label or indicator somewhere near the picker to hint that pressing 0 deselects.

### 7. Remove old bottom-left voxel HUD elements

**Files:** `src/main.rs`

Remove the old "Active: {voxel_name}" text and the old 48×48 single-tile preview that were drawn in the bottom-left corner. The mode text ("Mode: FLY"/"Mode: WALK") can remain in the bottom-left or be relocated as desired — it is not part of the tile picker.

Account for the picker height in layout so it doesn't overlap with other HUD elements. Ensure the picker background extends below the tiles to encompass the key labels.
