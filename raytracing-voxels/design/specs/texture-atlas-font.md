# Texture Atlas and Font Rendering

**Summary:** Add a texture atlas system for packing named images into a single GPU texture, extend the overlay system with atlas-aware drawing helpers, and implement bitmap font rendering using `ab_glyph` to rasterize glyphs into the atlas with proper kerning.
**Depends on:** immediate-mode-2d-overlay (Phase 4)

---

## Steps

### 5.1 TextureAtlas — packing and UV lookup

**Files:** `src/texture_atlas.rs`, `src/main.rs`

Implement a CPU-side texture atlas that packs named images into a single `Texture`:

- `pub struct TextureAtlas { texture: Texture, regions: HashMap<String, AtlasRegion> }`
- `pub struct AtlasRegion { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }` — pixel rect within the atlas.
- `pub struct TextureAtlasBuilder { entries: Vec<(String, Texture)> }` — collects named images before packing.
- `TextureAtlasBuilder::new()` — empty builder.
- `TextureAtlasBuilder::add(&mut self, name: impl Into<String>, image: Texture)` — adds a named image.
- `TextureAtlasBuilder::build(self) -> Result<TextureAtlas>` — packs all images into a single atlas texture using a simple shelf/row packing algorithm:
  - Sort entries by height descending.
  - Place images left-to-right in rows; when a row is full, start a new row below.
  - Atlas dimensions should be power-of-two, starting at 256x256 and doubling as needed until everything fits (cap at 4096x4096, return error if exceeded).
  - Blit each source image's pixels into the atlas texture at the assigned position.
- `TextureAtlas::region(&self, name: &str) -> Option<&AtlasRegion>` — looks up a named region.
- `TextureAtlas::uv_rect(&self, name: &str) -> Option<([f32; 2], [f32; 2])>` — returns `(uv_min, uv_max)` in normalized 0..1 coordinates for the named region.
- `TextureAtlas::texture(&self) -> &Texture` — returns the backing texture for GPU upload.
- Add `mod texture_atlas;` to `main.rs`.
- **Tests:**
  - Single image packs correctly, `region()` returns correct pixel coords.
  - `uv_rect()` returns normalized coordinates matching the region.
  - Multiple images of different sizes all pack without overlap.
  - Looking up a nonexistent name returns `None`.
  - Builder with no entries produces an empty atlas.

### 5.2 DrawList atlas helper

**Files:** `src/overlay.rs`

Add a convenience method on `DrawList` for drawing atlas regions:

- `pub fn atlas_rect(&mut self, x: f32, y: f32, w: f32, h: f32, uv_min: [f32; 2], uv_max: [f32; 2], color: Rgba)` — this is just an alias for `rect()` to make call sites more readable when working with atlas UV coords. The caller passes UVs obtained from `TextureAtlas::uv_rect()`.
- `pub fn atlas_sprite(&mut self, atlas: &TextureAtlas, name: &str, x: f32, y: f32, scale: f32, tint: Rgba) -> bool` — looks up the named region, computes `w = region.width * scale`, `h = region.height * scale`, calls `rect()` with the atlas UVs. Returns `false` if the name is not found.
- **Tests:**
  - `atlas_sprite` with a valid name produces 4 vertices and 6 indices.
  - `atlas_sprite` with an invalid name returns `false` and produces no geometry.

### 5.3 TextureAtlasFont — glyph rasterization

**Files:** `src/texture_atlas_font.rs`, `src/main.rs`, `Cargo.toml`

Implement font rendering by rasterizing glyphs into a texture atlas:

- Add `ab_glyph = "0.2"` to `Cargo.toml` dependencies.
- `pub struct GlyphInfo { pub atlas_name: String, pub advance_width: f32, pub left_side_bearing: f32, pub ascent_offset: f32, pub width: f32, pub height: f32 }` — per-glyph metrics.
- `pub struct TextureAtlasFont { atlas: TextureAtlas, glyphs: HashMap<char, GlyphInfo>, line_height: f32, ascent: f32, font: ab_glyph::FontArc }` — owns the atlas and glyph lookup table, plus the font for kerning queries.
- `TextureAtlasFont::new(font_data: &[u8], px_size: f32, charset: &str) -> Result<Self>`:
  - Parse the font with `ab_glyph::FontArc::try_from_slice`.
  - Create a `TextureAtlasBuilder`.
  - For each character in `charset`:
    - Get the scaled glyph at `px_size`.
    - Get `h_advance`, `h_side_bearing` from `ScaleFont`.
    - Outline the glyph; if it has an outline, rasterize it to a `Texture` (grayscale alpha — `Rgba { r: 255, g: 255, b: 255, a: coverage * 255 }`).
    - Add the rasterized image to the atlas builder with name = the character as a string.
    - Store `GlyphInfo` with the metrics and rasterized dimensions.
  - Build the atlas.
  - Compute `line_height` and `ascent` from `ScaleFont::height()` and `ScaleFont::ascent()`.
- `TextureAtlasFont::atlas(&self) -> &TextureAtlas` — accessor for GPU upload.
- Add `mod texture_atlas_font;` to `main.rs`.
- **Tests:**
  - Creating a font with a single character 'A' succeeds and the atlas contains a region for "A".
  - `line_height` and `ascent` are positive non-zero values.

### 5.4 TextureAtlasFont — text geometry generation

**Files:** `src/texture_atlas_font.rs`

Add methods to produce overlay geometry from text strings:

- `pub fn draw_text(&self, draw_list: &mut DrawList, text: &str, x: f32, y: f32, color: Rgba)`:
  - Start cursor at `(x, y)`. `y` represents the top of the line (cursor_y = y + ascent positions the baseline).
  - For each character:
    - Look up `GlyphInfo`. If not found (e.g., space with no glyph), advance by the space advance width and continue.
    - Compute kerning between previous and current character using `ab_glyph::ScaleFont::kern()`.
    - Apply kerning to cursor_x.
    - Compute the glyph's screen rect from `GlyphInfo` metrics (position offset by `left_side_bearing` and `ascent_offset`).
    - Get UVs from the atlas via `atlas.uv_rect(&glyph.atlas_name)`.
    - Call `draw_list.rect()` with the computed position, size, UVs, and color.
    - Advance cursor_x by `advance_width`.
- `pub fn text_width(&self, text: &str) -> f32` — computes the horizontal extent of a string (sum of advances + kerning), useful for centering.
- **Tests:**
  - `draw_text` with "AB" produces geometry (vertices > 0).
  - `text_width` of an empty string is 0.
  - `text_width` of "A" equals the advance width of 'A'.

### 5.5 Load Minecraft.otf font and render "Hello, World!"

**Files:** `src/main.rs`

Wire up the font system and replace the test overlay content:

- At the top of `main.rs`, embed the font: `const MINECRAFT_FONT: &[u8] = include_bytes!("../resources/Minecraft.otf");`
- Define the ASCII charset: all printable ASCII characters (0x20–0x7E).
- In `App`, replace `gradient_bind_group: Option<wgpu::BindGroup>` with `font: Option<TextureAtlasFont>` and `font_bind_group: Option<wgpu::BindGroup>`.
- In `try_resume`:
  - Create a `TextureAtlasFont` from `MINECRAFT_FONT` at a suitable pixel size (e.g., 32.0) with the ASCII charset.
  - Upload the font's atlas texture via `renderer.overlay().create_texture()` and store the bind group.
  - Remove the gradient texture creation code.
- In `RedrawRequested`:
  - Remove the three test `rect()` calls.
  - Instead, call `font.draw_text(&mut draw_list, "Hello, World!", 50.0, 50.0, Rgba::WHITE)`.
  - Render the draw list with the font's atlas bind group.
- Remove the `gradient_bind_group` field and all references to it.
- Verify the text renders correctly on screen with proper spacing and kerning.
