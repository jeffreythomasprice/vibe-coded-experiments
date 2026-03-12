# wgpu Raytracing Cube

**Summary:** Set up a wgpu+winit window rendering a full-screen quad with a WGSL fragment shader that ray-traces a unit cube at the origin. Camera parameters are passed via uniform buffer; Escape exits the window.
**Depends on:** None

---

## Steps

### 1.1 Winit window + event loop with Escape handling

**Files:** `src/main.rs`

Replace the hello-world with a winit `ApplicationHandler` implementation:

- Create a `struct App` that holds `Option<winit::window::Window>`.
- Implement `ApplicationHandler` for `App`:
  - `resumed`: create the window (or recreate if already exists).
  - `window_event`: handle `CloseRequested` and `KeyboardInput` where `key == Key::Named(NamedKey::Escape)` with `event_loop.exit()`.
  - `window_event(RedrawRequested)`: placeholder that just requests another redraw (will be filled in later steps).
- Run with `EventLoop::new()` + `event_loop.run_app(&mut app)`.
- At this point the app should open a window and close on Escape or window-close button.

### 1.2 Initialize wgpu surface, device, and queue

**Files:** `src/main.rs` (or extract to `src/renderer.rs`)

In the `resumed` handler, after creating the window:

- Create `wgpu::Instance` (default backends).
- Create a `wgpu::Surface` from the window (using `instance.create_surface(&window)`).
- Request an adapter (`instance.request_adapter`) with `compatible_surface`.
- Request a device + queue from the adapter (default limits/features).
- Configure the surface with the window's inner size and a preferred format (`surface.get_capabilities(&adapter).formats[0]`).
- Store `surface`, `device`, `queue`, and `surface_config` in a `struct Renderer` held as `Option<Renderer>` inside `App`.
- On `Resized` events, update the surface config and reconfigure.

Add dependencies to `Cargo.toml`:
- `pollster` (for blocking on async adapter/device requests)
- `env_logger` + `log` (for wgpu debug logging)

### 1.3 Full-screen quad render pipeline

**Files:** `src/renderer.rs`, `src/shader.wgsl`

Create the render pipeline that draws a full-screen triangle (or two-triangle quad):

- **Vertex shader** (`shader.wgsl`): Use a vertex shader with no vertex buffer — generate positions from `vertex_index` (the standard 3-vertex full-screen triangle trick: vertices at `(-1,-1)`, `(3,-1)`, `(-1,3)` with `vertex_index` 0,1,2). Output `@builtin(position)` and a `uv: vec2<f32>` varying in `[0,1]` range.
- **Fragment shader** (`shader.wgsl`): For now, output a solid color or UV-based gradient (will be replaced in step 1.5).
- **Pipeline**: Create a `wgpu::RenderPipeline` with:
  - The WGSL shader module.
  - No vertex buffers.
  - Single color target matching the surface format.
  - Primitive topology: `TriangleList`.
- In the `RedrawRequested` handler:
  - `surface.get_current_texture()` to get the frame.
  - Create a `TextureView` from it.
  - Begin a render pass with a clear color, execute the pipeline, `draw(0..3, 0..1)`.
  - Submit and present.

### 1.4 Camera uniform buffer and bind group

**Files:** `src/renderer.rs`, `src/shader.wgsl`

Define the camera parameters and pass them to the shader:

- **Rust side**: Define a `#[repr(C)]` struct `CameraUniforms` with fields:
  - `origin: [f32; 3]` — camera position in world space
  - `_pad0: f32`
  - `forward: [f32; 3]` — normalized forward direction
  - `_pad1: f32`
  - `right: [f32; 3]` — normalized right direction
  - `_pad2: f32`
  - `up: [f32; 3]` — normalized up direction
  - `fov: f32` — vertical field of view in radians
  - `aspect: f32` — width/height ratio
  - `_pad3: [f32; 3]`
- Derive or implement `bytemuck::Pod` + `bytemuck::Zeroable` for GPU upload.
- Create a `wgpu::Buffer` with `BufferUsages::UNIFORM | BufferUsages::COPY_DST`.
- Create a `BindGroupLayout` (binding 0, fragment visibility, uniform buffer) and a `BindGroup`.
- Set the pipeline layout to use this bind group layout.
- Each frame, write the camera uniforms to the buffer via `queue.write_buffer`.
- Initialize with a default camera at position `(2, 1.5, 3)` looking at the origin, FOV ~60 degrees.

Add `bytemuck` (with `derive` feature) to `Cargo.toml`.

### 1.5 Ray-tracing fragment shader for a unit cube

**Files:** `src/shader.wgsl`

Replace the placeholder fragment shader with a ray-tracing implementation:

- **Uniform block**: Declare a `struct Camera` matching `CameraUniforms` and bind it at `@group(0) @binding(0)`.
- **Ray construction**: From the fragment UV (remapped to `[-1,1]` with aspect correction), compute a ray origin and direction using the camera basis vectors and FOV.
- **Ray-box intersection**: Implement a slab-method ray-AABB intersection test against the box `min=(-0.5,-0.5,-0.5)`, `max=(0.5,0.5,0.5)`.
- **Shading**:
  - If the ray misses the box, output a sky/background color (e.g., gradient based on ray direction Y).
  - If the ray hits, compute the hit point and determine which face was hit (based on which slab was the entry plane).
  - Assign a distinct color per face or use simple diffuse lighting with a directional light (e.g., `normalize(vec3(1,2,3))`), using the face normal dot light direction.
- **Output**: `@location(0) vec4<f32>` color.

### 1.6 Window resize handling and aspect ratio update

**Files:** `src/renderer.rs` or `src/main.rs`

Ensure the renderer handles window resizes correctly:

- On `WindowEvent::Resized(new_size)`, reconfigure the surface with the new dimensions.
- Update `CameraUniforms.aspect` to `new_width / new_height`.
- Skip frames where either dimension is 0 (minimized window).
