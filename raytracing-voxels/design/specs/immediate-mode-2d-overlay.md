# Immediate-Mode 2D Overlay Renderer

**Summary:** Add a second render pass that draws textured 2D geometry in window-space coordinates using an immediate-mode API, layered on top of the existing voxel raytracing pass.
**Depends on:** wgpu-raytracing-cube (Phase 1), voxel-chunk-raytracing (Phase 3)

---

## Steps

### 4.1 RGBA type and texture data structure

**Files:** `src/overlay.rs`, `src/main.rs`

Define the core pixel and texture types:

- `#[repr(C)] #[derive(Clone, Copy, Pod, Zeroable)] pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }` — a single RGBA pixel. Derive `bytemuck::Pod` and `Zeroable`.
- Add convenience constructors: `Rgba::new(r, g, b, a)`, `Rgba::rgb(r, g, b)` (sets a=255), `Rgba::WHITE`, `Rgba::BLACK` as constants.
- `pub struct Texture { width: u32, height: u32, pixels: Vec<Rgba> }` — CPU-side texture data.
- `Texture::new(width, height)` — creates a texture filled with transparent black.
- `Texture::set_pixel(&mut self, x: u32, y: u32, color: Rgba)` — sets a pixel. No-op if out of bounds.
- `Texture::get_pixel(&self, x: u32, y: u32) -> Rgba` — returns the pixel. Returns transparent black if out of bounds.
- `Texture::data(&self) -> &[Rgba]` — returns the pixel slice for GPU upload.
- `Texture::width(&self)` / `Texture::height(&self)` accessors.
- Add `mod overlay;` to `main.rs`.
- **Tests:**
  - `Rgba::rgb(255, 0, 0)` has a=255.
  - `Texture::new` is all-zero.
  - `set_pixel`/`get_pixel` round-trip.
  - Out-of-bounds access returns transparent black / is a no-op.

### 4.2 Overlay vertex type and batch buffer

**Files:** `src/overlay.rs`

Define the vertex format and the immediate-mode draw list:

- `#[repr(C)] #[derive(Clone, Copy, Pod, Zeroable)] pub struct OverlayVertex { pub pos: [f32; 2], pub color: [f32; 4], pub uv: [f32; 2] }` — position in window pixels, RGBA color as floats (0.0–1.0), texture UV.
- Implement `wgpu::VertexBufferLayout` for `OverlayVertex` with attributes: position (Float32x2), color (Float32x4), uv (Float32x2). Provide this via a `pub fn vertex_layout() -> wgpu::VertexBufferLayout<'static>` function.
- `pub struct DrawList { vertices: Vec<OverlayVertex>, indices: Vec<u32> }` — collects geometry each frame.
- `DrawList::new()` — empty.
- `DrawList::clear()` — resets for next frame.
- `DrawList::vertex_data(&self) -> &[u8]` — returns vertices as bytes via `bytemuck::cast_slice`.
- `DrawList::index_data(&self) -> &[u8]` — returns indices as bytes.
- `DrawList::index_count(&self) -> u32`.
- **Tests:**
  - `DrawList::new()` has zero vertices and indices.
  - `clear()` empties the lists.

### 4.3 Immediate-mode rect API

**Files:** `src/overlay.rs`

Add the public drawing interface on `DrawList`:

- `pub fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, uv_min: [f32; 2], uv_max: [f32; 2], color: Rgba)` — appends a textured, tinted rectangle. Coordinates are in window pixels (origin top-left). Color is applied as a tint (multiplied with texture in the shader).
  - Emits 4 vertices and 6 indices (two triangles).
  - Vertices at corners: (x,y), (x+w,y), (x+w,y+h), (x,y+h) with corresponding UVs.
  - Color is converted from `Rgba` u8 to `[f32; 4]` (divide by 255.0).
- `pub fn solid_rect(&mut self, x: f32, y: f32, w: f32, h: f32, color: Rgba)` — convenience that calls `rect` with uv_min=[0,0], uv_max=[1,1]. When used with a 1x1 white texture, this produces a solid colored rectangle.
- **Tests:**
  - `rect()` appends exactly 4 vertices and 6 indices.
  - Two `rect()` calls produce 8 vertices and 12 indices with correct index offsets (second quad's indices start at 4).
  - Vertex positions match the supplied x/y/w/h.

### 4.4 Overlay GPU renderer — pipeline and resources

**Files:** `src/overlay_renderer.rs`, `src/main.rs`

Create the wgpu resources for the overlay pass:

- New file `src/overlay_renderer.rs`. Add `mod overlay_renderer;` to `main.rs`.
- `pub struct OverlayRenderer` with fields:
  - `pipeline: wgpu::RenderPipeline`
  - `vertex_buffer: wgpu::Buffer` — dynamic, recreated/grown as needed.
  - `index_buffer: wgpu::Buffer` — dynamic, recreated/grown as needed.
  - `sampler: wgpu::Sampler`
  - `white_texture: wgpu::Texture` + `white_bind_group: wgpu::BindGroup` — 1x1 white pixel, used as default texture.
  - `bind_group_layout: wgpu::BindGroupLayout` — for texture + sampler.
  - `screen_uniform_buffer: wgpu::Buffer` — holds `[f32; 2]` for screen size.
  - `screen_bind_group: wgpu::BindGroup`
  - `vertex_capacity: u64`, `index_capacity: u64` — current buffer sizes.
- `OverlayRenderer::new(device, queue, surface_format) -> Result<Self>`:
  - Create the shader module from `src/overlay.wgsl`.
  - Create bind group layout with: group 0 = screen size uniform; group 1 = texture (2d, float, sampled) + sampler.
  - Create the 1x1 white texture and its bind group.
  - Create the render pipeline with alpha blending enabled (`SrcAlpha` / `OneMinusSrcAlpha`), triangle list topology, and the `OverlayVertex` buffer layout.
  - Initial vertex/index buffers with a reasonable default capacity (e.g., 1024 vertices, 2048 indices).
- `pub fn create_texture(&self, device, queue, texture: &Texture) -> wgpu::BindGroup` — uploads a `Texture` to the GPU and returns a bind group for it. Creates a `wgpu::Texture` with `Rgba8UnormSrgb` format, writes the pixel data, creates a `TextureView` and returns a `BindGroup` using the shared layout.
- `pub fn resize(&mut self, queue, width, height)` — updates the screen-size uniform.

### 4.5 Overlay GPU renderer — draw submission

**Files:** `src/overlay_renderer.rs`

Add the per-frame render method:

- `pub fn render(&mut self, device, queue, view: &wgpu::TextureView, encoder: &mut wgpu::CommandEncoder, draw_list: &DrawList, texture_bind_group: &wgpu::BindGroup)`:
  - If `draw_list.index_count() == 0`, return early.
  - Grow vertex/index buffers if the draw list exceeds current capacity. Create new buffers and update the stored capacity.
  - Write vertex and index data to buffers via `queue.write_buffer`.
  - Begin a render pass on `view` with `LoadOp::Load` (preserves the voxel pass output) and `StoreOp::Store`.
  - Set pipeline, bind groups (group 0 = screen uniform, group 1 = texture), vertex buffer, index buffer.
  - `draw_indexed(0..draw_list.index_count(), 0, 0..1)`.
- This design takes a single texture bind group per draw call. For the test scenario this is sufficient (one gradient texture applied to all rects).

### 4.6 Overlay shader

**Files:** `src/overlay.wgsl`

Write the WGSL shader for 2D overlay rendering:

- Vertex shader (`vs_main`):
  - Input: `pos: vec2<f32>` (window pixels), `color: vec4<f32>`, `uv: vec2<f32>`.
  - Uniform group 0 binding 0: `screen_size: vec2<f32>`.
  - Convert window pixel coords to clip space: `clip.x = (pos.x / screen_size.x) * 2.0 - 1.0`, `clip.y = 1.0 - (pos.y / screen_size.y) * 2.0` (top-left origin).
  - Output: `@builtin(position)`, `color`, `uv`.
- Fragment shader (`fs_main`):
  - Group 1 binding 0: `texture_2d<f32>`, binding 1: `sampler`.
  - Sample the texture at UV, multiply by the vertex color (component-wise).
  - Output the resulting `vec4<f32>`.

### 4.7 Split rendering into two passes

**Files:** `src/renderer.rs`, `src/main.rs`

Refactor the render loop so the voxel pass and overlay pass share a frame:

- Change `Renderer::render` to accept a `&wgpu::TextureView` and `&mut wgpu::CommandEncoder` instead of managing its own. Alternatively, split it so it returns the frame texture and the caller orchestrates passes. The simplest approach:
  - Add `pub fn begin_frame(&mut self) -> Option<(wgpu::SurfaceTexture, wgpu::TextureView)>` — gets the current texture, creates the view, handles `SurfaceError::Lost`.
  - Change `render` to `pub fn render_voxels(&mut self, encoder: &mut wgpu::CommandEncoder, view: &wgpu::TextureView, camera: &CameraUniforms)` — runs the existing full-screen quad pass (with `LoadOp::Clear`).
  - Add `pub fn submit(&self, encoder: wgpu::CommandEncoder)` — calls `self.queue.submit`.
  - Add `pub fn device(&self) -> &wgpu::Device` and `pub fn queue(&self) -> &wgpu::Queue` accessors so the overlay renderer can use them.
- Store `OverlayRenderer` in `Renderer` (created during `Renderer::new`).
- Expose `pub fn overlay(&mut self) -> &mut OverlayRenderer` accessor.
- Update `main.rs` `RedrawRequested` to:
  1. `begin_frame()` to get texture/view.
  2. Create command encoder.
  3. `render_voxels(encoder, view, camera)` — clears and draws voxels.
  4. `overlay_renderer.render(device, queue, view, encoder, draw_list, texture_bg)` — draws overlay on top.
  5. `submit(encoder)` and `present()`.

### 4.8 Procedural gradient texture and test rectangles

**Files:** `src/main.rs`

Wire up the test content to prove the system works:

- Generate a procedural gradient texture (e.g., 256x256): for each pixel, compute a black-to-white value based on the x coordinate (`brightness = x * 255 / width`), producing a horizontal gradient. Use `Rgba::rgb(b, b, b)`.
- At startup (in `try_resume`), create the gradient texture, upload it via `overlay_renderer.create_texture()`, and store the bind group.
- In `RedrawRequested`, after the voxel pass, build the overlay `DrawList`:
  - A rectangle at e.g. (50, 50) size 200x200, tinted red `Rgba::rgb(255, 0, 0)`, with the gradient texture.
  - A rectangle at e.g. (300, 50) size 200x200, tinted green `Rgba::rgb(0, 255, 0)`, with the gradient texture.
  - A rectangle at e.g. (550, 50) size 200x200, tinted blue `Rgba::rgb(0, 0, 255)`, with the gradient texture.
- Render the draw list with the gradient texture bind group.
- This produces three colored rectangles with a visible gradient pattern, proving textures, tinting, and window-space positioning all work.
