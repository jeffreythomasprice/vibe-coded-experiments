mod bvh;
mod camera;
mod chunk;
mod chunk_manager;
mod config;
mod mesh_catalog;
mod overlay;
mod overlay_renderer;
mod player;
mod seed;
mod terrain;
mod texture_atlas;
mod texture_atlas_font;
mod voxel_renderer;
mod voxel_textures;
mod world;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use glam::{IVec3, Vec2, Vec3};
use tracing::{error, info, trace};
use voxel_renderer::{GpuInteractionState, GpuLightingUniforms, GpuPointLight};

use camera::Camera;
use chunk_manager::ChunkManager;
use config::Config;
use overlay::{DrawList, Rgba, Texture};
use player::{InputState, Player};
use texture_atlas_font::TextureAtlasFont;
use voxel_renderer::Renderer;
use world::World;

const VOXEL_TYPE_NAMES: &[&str] = &["", "stone", "dirt", "grass", "brick", "wood", "leaves", "water"];
const VOXEL_KEY_TO_ID: [u8; 8] = [0, 3, 2, 1, 4, 5, 6, 7];
const TILE_PICKER_TILE_IDS: [u8; 7] = [
    VOXEL_KEY_TO_ID[1], // key 1: grass
    VOXEL_KEY_TO_ID[2], // key 2: dirt
    VOXEL_KEY_TO_ID[3], // key 3: stone
    VOXEL_KEY_TO_ID[4], // key 4: brick
    VOXEL_KEY_TO_ID[5], // key 5: wood
    VOXEL_KEY_TO_ID[6], // key 6: leaves
    VOXEL_KEY_TO_ID[7], // key 7: water
];

const TILE_PICKER_TILE_SIZE: f32 = 48.0;
const TILE_PICKER_SPACING: f32 = 8.0;
const TILE_PICKER_PADDING: f32 = 12.0;
const TILE_PICKER_BORDER: f32 = 3.0;
const TILE_PICKER_BG_COLOR: Rgba = Rgba::new(30, 30, 30, 200);
const TILE_PICKER_BORDER_COLOR: Rgba = Rgba::new(80, 80, 80, 220);
const TILE_PICKER_HIGHLIGHT_COLOR: Rgba = Rgba::new(255, 255, 100, 180);

const MINECRAFT_FONT: &[u8] = include_bytes!("../resources/Minecraft.otf");
use winit::application::ApplicationHandler;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const MOUSE_SENSITIVITY: f32 = 0.003;
const MAX_DT: f32 = 0.1;
const BREAK_TIME: f32 = 0.4;
const INTERACT_REACH: f32 = 6.0;

struct BreakState {
    world_pos: IVec3,
    elapsed: f32,
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,
    camera: Camera,
    player: Player,
    world: World,
    chunk_manager: ChunkManager,
    input: InputState,
    last_frame: Instant,
    cursor_grabbed: bool,
    font: Option<TextureAtlasFont>,
    font_bind_group: Option<wgpu::BindGroup>,
    draw_list: DrawList,
    hud_draw_list: DrawList,
    crosshair_draw_list: DrawList,
    frame_count: u32,
    fps_accum: f32,
    fps_display: f32,
    active_voxel_type: Option<u8>,
    break_state: Option<BreakState>,
    crosshair_bind_group: Option<wgpu::BindGroup>,
    voxel_atlas_overlay_bind_group: Option<wgpu::BindGroup>,
    solid_bind_group: Option<wgpu::BindGroup>,
    picker_draw_list: DrawList,
    voxel_uv_map: [[f32; 4]; 256],
    last_save: Instant,
    storage_dir: PathBuf,
    seed: u32,
    last_space_press: Option<Instant>,
    mesh_catalog: mesh_catalog::MeshCatalog,
    point_lights: Vec<GpuPointLight>,
    point_lights_dirty: bool,
    sun_angle: f32,
    ambient: f32,
    sun_intensity: f32,
    day_duration_secs: f32,
}

impl App {
    fn new(storage_dir: PathBuf, seed: u32, config: &Config) -> Self {
        let mut world = World::new();
        let mut chunk_manager = ChunkManager::new(6, 2197, storage_dir.clone(), seed);

        let camera = match Camera::load_from_file(&camera::camera_file_path(&storage_dir)) {
            Ok(cam) => {
                info!("loaded camera from {}", storage_dir.display());
                cam
            }
            Err(_) => {
                let spawn_x = 24;
                let spawn_z = 40;
                let terrain_gen = terrain::TerrainGenerator::new(seed);
                let surface_y = terrain_gen.surface_height(spawn_x, spawn_z);
                let feet_y = (surface_y + 1) as f32;
                let cam_pos = Vec3::new(spawn_x as f32, feet_y + player::EYE_HEIGHT, spawn_z as f32);
                Camera::new(
                    cam_pos,
                    0.0,
                    0.0,
                    60.0_f32.to_radians(),
                )
            }
        };

        info!(position = ?camera.position, "initial camera position");
        chunk_manager.load_initial_sync(camera.position, &mut world, 2);
        info!(chunks = world.chunk_count(), "initial chunks loaded synchronously");

        let player = Player::new(camera.position);

        Self {
            window: None,
            renderer: None,
            camera,
            player,
            world,
            chunk_manager,
            input: InputState::default(),
            last_frame: Instant::now(),
            cursor_grabbed: false,
            font: None,
            font_bind_group: None,
            draw_list: DrawList::new(),
            hud_draw_list: DrawList::new(),
            crosshair_draw_list: DrawList::new(),
            frame_count: 0,
            fps_accum: 0.0,
            fps_display: 0.0,
            active_voxel_type: Some(VOXEL_KEY_TO_ID[1]), // default: grass
            break_state: None,
            crosshair_bind_group: None,
            voxel_atlas_overlay_bind_group: None,
            solid_bind_group: None,
            picker_draw_list: DrawList::new(),
            voxel_uv_map: [[0.0; 4]; 256],
            last_save: Instant::now(),
            storage_dir,
            seed,
            last_space_press: None,
            mesh_catalog: mesh_catalog::MeshCatalog::build(),
            point_lights: Vec::new(),
            point_lights_dirty: false,
            sun_angle: config.sun_angle,
            ambient: config.ambient,
            sun_intensity: config.sun_intensity,
            day_duration_secs: config.day_duration_secs,
        }
    }

    fn grab_cursor(&mut self) {
        if let Some(window) = &self.window
            && window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| window.set_cursor_grab(CursorGrabMode::Confined))
                .is_ok()
        {
            window.set_cursor_visible(false);
            self.cursor_grabbed = true;
        }
    }

    fn release_cursor(&mut self) {
        if let Some(window) = &self.window {
            let _ = window.set_cursor_grab(CursorGrabMode::None);
            window.set_cursor_visible(true);
            self.cursor_grabbed = false;
            self.break_state = None;
        }
    }
}

impl App {
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        info!("initializing renderer and window");
        let window_attrs = Window::default_attributes().with_title("Raytracing Voxels");
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .context("failed to create window")?,
        );
        let mut renderer = Renderer::new(window.clone())?;
        let (voxel_data, chunk_infos, bvh_nodes) = self.world.pack_gpu_data();
        renderer.upload_world(
            &voxel_data,
            &chunk_infos,
            chunk_infos.len() as u32,
            &bvh_nodes,
        );
        self.world.clear_dirty();

        let atlas = voxel_textures::build_voxel_atlas(self.seed)?;
        renderer.upload_voxel_atlas(atlas.texture(), atlas.uv_map());
        self.voxel_uv_map = *atlas.uv_map();

        renderer.upload_mesh_catalog(&self.mesh_catalog);

        self.voxel_atlas_overlay_bind_group = Some(renderer.overlay().create_texture(
            renderer.device(),
            renderer.queue(),
            atlas.texture(),
        ));

        let crosshair_tex = generate_crosshair_texture();
        self.crosshair_bind_group = Some(renderer.overlay().create_texture(
            renderer.device(),
            renderer.queue(),
            &crosshair_tex,
        ));

        let ascii_charset: String = (0x20u8..=0x7E).map(|b| b as char).collect();
        let font = TextureAtlasFont::new(MINECRAFT_FONT, 28.0, &ascii_charset)
            .context("failed to create font")?;
        self.font_bind_group = Some(renderer.overlay().create_texture(
            renderer.device(),
            renderer.queue(),
            font.atlas().texture(),
        ));
        self.font = Some(font);

        let mut solid_tex = Texture::new(1, 1);
        solid_tex.set_pixel(0, 0, Rgba::WHITE);
        self.solid_bind_group = Some(renderer.overlay().create_texture(
            renderer.device(),
            renderer.queue(),
            &solid_tex,
        ));

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last_frame = Instant::now();
        info!("renderer initialization complete");
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.try_resume(event_loop) {
            error!("failed to initialize: {e:#}");
            event_loop.exit();
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: DeviceId,
        event: DeviceEvent,
    ) {
        if !self.cursor_grabbed {
            return;
        }
        if let DeviceEvent::MouseMotion { delta: (dx, dy) } = event {
            self.camera.rotate(
                -dx as f32 * MOUSE_SENSITIVITY,
                -dy as f32 * MOUSE_SENSITIVITY,
            );
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("window close requested, saving state");
                let cam_path = camera::camera_file_path(&self.storage_dir);
                if let Err(e) = self.camera.save_to_file(&cam_path) {
                    error!("failed to save camera: {e:#}");
                }
                self.chunk_manager.save_all_modified(&mut self.world);
                event_loop.exit();
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } => {
                if self.cursor_grabbed {
                    if let Some(hit) = self.world.raycast(
                        self.camera.position,
                        self.camera.forward(),
                        INTERACT_REACH,
                        Some(&self.mesh_catalog),
                    ) && hit.voxel_id != 0
                    {
                        let hit_world = hit.chunk_pos * 16
                            + IVec3::new(
                                hit.local_pos[0] as i32,
                                hit.local_pos[1] as i32,
                                hit.local_pos[2] as i32,
                            );
                        if hit.voxel_id >= mesh_catalog::MESH_VOXEL_BASE {
                            // Mesh voxel (e.g. torch): instant removal
                            self.world.set_voxel(hit_world, 0);
                            let voxel_center = Vec3::new(
                                hit_world.x as f32 + 0.5,
                                hit_world.y as f32 + 0.5,
                                hit_world.z as f32 + 0.5,
                            );
                            if let Some(idx) = self.point_lights.iter().position(|l| {
                                let lp = Vec3::from_array(l.position);
                                (lp - voxel_center).length() < 1.0
                            }) {
                                self.point_lights.remove(idx);
                                self.point_lights_dirty = true;
                            }
                        } else {
                            // Regular block: hold-to-break
                            self.break_state = Some(BreakState {
                                world_pos: hit_world,
                                elapsed: 0.0,
                            });
                        }
                    }
                } else {
                    self.grab_cursor();
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                self.break_state = None;
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } => {
                if self.cursor_grabbed
                    && let Some(voxel_id) = self.active_voxel_type
                    && let Some(hit) = self.world.raycast(
                        self.camera.position,
                        self.camera.forward(),
                        INTERACT_REACH,
                        Some(&self.mesh_catalog),
                    )
                {
                    let hit_world = hit.chunk_pos * 16
                        + IVec3::new(
                            hit.local_pos[0] as i32,
                            hit.local_pos[1] as i32,
                            hit.local_pos[2] as i32,
                        );
                    let place_pos = hit_world + hit.normal;
                    trace!(pos = ?place_pos, voxel_id, "placed voxel");
                    self.world.set_voxel(place_pos, voxel_id);
                    if voxel_id >= mesh_catalog::MESH_VOXEL_BASE {
                        // Mesh placement: also create a point light
                        self.point_lights.push(GpuPointLight {
                            position: [
                                place_pos.x as f32 + 0.5,
                                place_pos.y as f32 + 0.85,
                                place_pos.z as f32 + 0.5,
                            ],
                            radius: 8.0,
                            color: [1.0, 0.9, 0.7],
                            _padding: 0.0,
                        });
                        self.point_lights_dirty = true;
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.cursor_grabbed => {
                let scroll = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y as f64,
                    MouseScrollDelta::PixelDelta(pos) => pos.y / 40.0,
                };
                if scroll.abs() > 0.0 {
                    const ALL_SLOTS: [u8; 7] = [
                        TILE_PICKER_TILE_IDS[0],
                        TILE_PICKER_TILE_IDS[1],
                        TILE_PICKER_TILE_IDS[2],
                        TILE_PICKER_TILE_IDS[3],
                        TILE_PICKER_TILE_IDS[4],
                        TILE_PICKER_TILE_IDS[5],
                        mesh_catalog::MESH_TORCH,
                    ];
                    let current_idx = self
                        .active_voxel_type
                        .and_then(|id| ALL_SLOTS.iter().position(|&s| s == id));
                    let len = ALL_SLOTS.len() as i32;
                    let new_idx = if scroll > 0.0 {
                        // Scroll up = move left (previous slot)
                        match current_idx {
                            Some(i) => ((i as i32 - 1).rem_euclid(len)) as usize,
                            None => ALL_SLOTS.len() - 1,
                        }
                    } else {
                        // Scroll down = move right (next slot)
                        match current_idx {
                            Some(i) => ((i as i32 + 1).rem_euclid(len)) as usize,
                            None => 0,
                        }
                    };
                    self.active_voxel_type = Some(ALL_SLOTS[new_idx]);
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key: Key::Named(NamedKey::Escape),
                        state: ElementState::Pressed,
                        ..
                    },
                ..
            } => {
                if self.cursor_grabbed {
                    self.release_cursor();
                } else {
                    let cam_path = camera::camera_file_path(&self.storage_dir);
                    if let Err(e) = self.camera.save_to_file(&cam_path) {
                        error!("failed to save camera: {e:#}");
                    }
                    self.chunk_manager.save_all_modified(&mut self.world);
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
                        repeat,
                        ..
                    },
                ..
            } => {
                let pressed = state == ElementState::Pressed;
                match code {
                    KeyCode::KeyW => self.input.forward = pressed,
                    KeyCode::KeyS => self.input.back = pressed,
                    KeyCode::KeyA => self.input.left = pressed,
                    KeyCode::KeyD => self.input.right = pressed,
                    KeyCode::Space => {
                        self.input.up = pressed;
                        if pressed && !repeat {
                            let now = Instant::now();
                            if let Some(last) = self.last_space_press {
                                if now.duration_since(last).as_millis() < 300 {
                                    self.player.toggle_fly_mode();
                                    self.last_space_press = None;
                                } else {
                                    self.last_space_press = Some(now);
                                }
                            } else {
                                self.last_space_press = Some(now);
                            }
                        }
                    }
                    KeyCode::ShiftLeft => self.input.down = pressed,
                    KeyCode::Digit0 if pressed => self.active_voxel_type = None,
                    KeyCode::Digit1 if pressed => self.active_voxel_type = Some(VOXEL_KEY_TO_ID[1]),
                    KeyCode::Digit2 if pressed => self.active_voxel_type = Some(VOXEL_KEY_TO_ID[2]),
                    KeyCode::Digit3 if pressed => self.active_voxel_type = Some(VOXEL_KEY_TO_ID[3]),
                    KeyCode::Digit4 if pressed => self.active_voxel_type = Some(VOXEL_KEY_TO_ID[4]),
                    KeyCode::Digit5 if pressed => self.active_voxel_type = Some(VOXEL_KEY_TO_ID[5]),
                    KeyCode::Digit6 if pressed => self.active_voxel_type = Some(VOXEL_KEY_TO_ID[6]),
                    KeyCode::Digit7 if pressed => self.active_voxel_type = Some(VOXEL_KEY_TO_ID[7]),
                    KeyCode::Digit9 if pressed => self.active_voxel_type = Some(mesh_catalog::MESH_TORCH),
                    _ => {}
                }
            }
            WindowEvent::Resized(new_size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(new_size.width, new_size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                let dt = self.last_frame.elapsed().as_secs_f32().min(MAX_DT);
                self.last_frame = Instant::now();

                self.frame_count += 1;
                self.fps_accum += dt;
                if self.fps_accum >= 1.0 {
                    self.fps_display = self.frame_count as f32 / self.fps_accum;
                    self.frame_count = 0;
                    self.fps_accum = 0.0;
                }

                self.player
                    .tick(dt, &self.input, self.camera.yaw, &self.world);
                self.camera.position = self.player.eye_position();

                self.chunk_manager.update_camera(self.camera.position);
                self.chunk_manager.drain_results(&mut self.world);

                if self.last_save.elapsed().as_secs_f32() >= 5.0 {
                    trace!("periodic save tick");
                    self.chunk_manager.save_modified(&mut self.world);
                    let cam_path = camera::camera_file_path(&self.storage_dir);
                    if let Err(e) = self.camera.save_to_file(&cam_path) {
                        error!("failed to save camera: {e:#}");
                    }
                    self.last_save = Instant::now();
                }

                // Update break state
                if let Some(bs) = &mut self.break_state {
                    if let Some(hit) = self.world.raycast(
                        self.camera.position,
                        self.camera.forward(),
                        INTERACT_REACH,
                        Some(&self.mesh_catalog),
                    ) {
                        let hit_world = hit.chunk_pos * 16
                            + IVec3::new(
                                hit.local_pos[0] as i32,
                                hit.local_pos[1] as i32,
                                hit.local_pos[2] as i32,
                            );
                        if hit_world == bs.world_pos {
                            bs.elapsed += dt;
                            if bs.elapsed >= BREAK_TIME {
                                let pos = bs.world_pos;
                                trace!(?pos, "broke voxel");
                                self.world.set_voxel(pos, 0);
                                self.break_state = None;
                            }
                        } else {
                            self.break_state = None;
                        }
                    } else {
                        self.break_state = None;
                    }
                }

                // Build interaction state for GPU
                let eye_voxel_pos = IVec3::new(
                    self.camera.position.x.floor() as i32,
                    self.camera.position.y.floor() as i32,
                    self.camera.position.z.floor() as i32,
                );
                let is_submerged = if self.world.get_voxel(eye_voxel_pos) == world::WATER_VOXEL_ID { 1u32 } else { 0u32 };

                let mut interaction = GpuInteractionState {
                    highlight_pos: [0.0; 3],
                    highlight_active: 0,
                    break_pos: [0.0; 3],
                    break_progress: 0.0,
                    is_submerged,
                    _pad0: 0,
                    _pad1: 0,
                    _pad2: 0,
                };

                // Placement preview highlight
                if self.active_voxel_type.is_some()
                    && let Some(hit) = self.world.raycast(
                        self.camera.position,
                        self.camera.forward(),
                        INTERACT_REACH,
                        Some(&self.mesh_catalog),
                    )
                {
                    let hit_world = hit.chunk_pos * 16
                        + IVec3::new(
                            hit.local_pos[0] as i32,
                            hit.local_pos[1] as i32,
                            hit.local_pos[2] as i32,
                        );
                    let place_pos = hit_world + hit.normal;
                    interaction.highlight_pos =
                        [place_pos.x as f32, place_pos.y as f32, place_pos.z as f32];
                    interaction.highlight_active = 1;
                }

                // Break progress
                if let Some(bs) = &self.break_state {
                    interaction.break_pos = [
                        bs.world_pos.x as f32,
                        bs.world_pos.y as f32,
                        bs.world_pos.z as f32,
                    ];
                    interaction.break_progress = (bs.elapsed / BREAK_TIME).clamp(0.0, 1.0);
                }

                self.sun_angle += (std::f32::consts::TAU / self.day_duration_secs) * dt;
                let sun_y = self.sun_angle.sin();
                let sun_xz = self.sun_angle.cos();
                let sun_dir = Vec3::new(sun_xz, sun_y, 0.3).normalize();
                let time_of_day = (self.sun_angle / std::f32::consts::TAU).fract();

                let effective_intensity = if sun_y > 0.0 {
                    self.sun_intensity
                } else {
                    0.0
                };

                let effective_ambient = if sun_y > 0.1 {
                    self.ambient
                } else if sun_y > -0.1 {
                    let t = (sun_y + 0.1) / 0.2;
                    self.ambient * (0.3 + 0.7 * t)
                } else {
                    self.ambient * 0.3
                };

                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    renderer.upload_interaction(&interaction);

                    renderer.upload_lighting(&GpuLightingUniforms {
                        sun_dir: sun_dir.to_array(),
                        ambient: effective_ambient,
                        sun_color: [1.0, 1.0, 1.0],
                        sun_intensity: effective_intensity,
                        time_of_day,
                        _pad0: 0.0,
                        _pad1: 0.0,
                        _pad2: 0.0,
                    });

                    if self.point_lights_dirty {
                        renderer.upload_point_lights(&self.point_lights);
                        self.point_lights_dirty = false;
                    }

                    if self.world.is_dirty() {
                        let (voxel_data, chunk_infos, bvh_nodes) = self.world.pack_gpu_data();
                        renderer.upload_world(
                            &voxel_data,
                            &chunk_infos,
                            chunk_infos.len() as u32,
                            &bvh_nodes,
                        );
                        self.world.clear_dirty();
                    }

                    let uniforms = self.camera.to_uniforms(renderer.aspect());

                    if let Some((frame, view)) = renderer.begin_frame() {
                        let mut encoder = renderer.create_encoder();
                        renderer.render_voxels(&mut encoder, &view, &uniforms);

                        self.draw_list.clear();
                        let screen_width = renderer.screen_width();
                        let screen_height = renderer.screen_height();

                        if let Some(font) = &self.font {
                            let line_height = font.line_height();
                            let mut y = 10.0;
                            let color = Rgba::WHITE;

                            let fps_text = format!("FPS: {:.0}", self.fps_display);
                            font.draw_text(&mut self.draw_list, &fps_text, 10.0, y, color);
                            y += line_height;

                            let chunk_count = self.world.chunk_count();
                            let desired = self.chunk_manager.desired_count();
                            let max_loaded = self.chunk_manager.max_loaded();
                            font.draw_text(
                                &mut self.draw_list,
                                &format!(
                                    "Chunks: {} / {} (max {})",
                                    chunk_count, desired, max_loaded
                                ),
                                10.0,
                                y,
                                Rgba::WHITE,
                            );
                            y += line_height;

                            let cpu_chunk_bytes = chunk_count as u64 * 4096;
                            let font_atlas_bytes = self.font.as_ref().map_or(0u64, |f| {
                                let tex = f.atlas().texture();
                                tex.width() as u64 * tex.height() as u64 * 4
                            });
                            let total_cpu = cpu_chunk_bytes + font_atlas_bytes;

                            let gpu_stats = renderer.gpu_memory_stats();
                            let total_gpu = gpu_stats.voxel_buffer_bytes
                                + gpu_stats.chunk_info_bytes
                                + gpu_stats.chunk_count_bytes
                                + gpu_stats.bvh_node_bytes
                                + gpu_stats.bvh_count_bytes
                                + gpu_stats.voxel_atlas_bytes
                                + font_atlas_bytes
                                + 32 * 32 * 4; // crosshair

                            font.draw_text(
                                &mut self.draw_list,
                                &format!(
                                    "Mem: CPU {} | GPU {}",
                                    format_bytes(total_cpu),
                                    format_bytes(total_gpu),
                                ),
                                10.0,
                                y,
                                Rgba::WHITE,
                            );
                            y += line_height;

                            let mode_text = if self.player.fly_mode {
                                "Mode: FLY"
                            } else {
                                "Mode: WALK"
                            };
                            font.draw_text(&mut self.draw_list, mode_text, 10.0, y, Rgba::WHITE);
                            y += line_height;

                            let hour = (time_of_day * 24.0) as u32;
                            let minute = ((time_of_day * 24.0).fract() * 60.0) as u32;
                            let time_text = format!("Time: {:02}:{:02}", hour, minute);
                            font.draw_text(&mut self.draw_list, &time_text, 10.0, y, Rgba::WHITE);
                        }

                        if let Some(texture_bg) = &self.font_bind_group {
                            renderer.render_overlay(
                                &view,
                                &mut encoder,
                                &self.draw_list,
                                texture_bg,
                            );
                        }

                        // Crosshair overlay
                        self.crosshair_draw_list.clear();
                        self.crosshair_draw_list.rect(
                            screen_width / 2.0 - 16.0,
                            screen_height / 2.0 - 16.0,
                            32.0,
                            32.0,
                            Vec2::ZERO,
                            Vec2::ONE,
                            Rgba::WHITE,
                        );
                        if let Some(crosshair_bg) = &self.crosshair_bind_group {
                            renderer.render_overlay(
                                &view,
                                &mut encoder,
                                &self.crosshair_draw_list,
                                crosshair_bg,
                            );
                        }

                        // Tile picker (6 block tiles + separator gap + 1 torch slot)
                        let num_tiles = TILE_PICKER_TILE_IDS.len() as f32;
                        let torch_separator = TILE_PICKER_SPACING * 2.0;
                        let num_total_slots = num_tiles + 1.0; // +1 for torch
                        let picker_inner_w = num_total_slots * TILE_PICKER_TILE_SIZE
                            + (num_tiles - 1.0) * TILE_PICKER_SPACING
                            + torch_separator
                            + 2.0 * TILE_PICKER_PADDING;
                        let label_extra = 20.0;
                        let picker_inner_h =
                            TILE_PICKER_TILE_SIZE + 2.0 * TILE_PICKER_PADDING + label_extra;
                        let picker_x = (screen_width - picker_inner_w) / 2.0 - TILE_PICKER_BORDER;
                        let picker_y = screen_height - picker_inner_h - TILE_PICKER_BORDER - 20.0;
                        let picker_outer_w = picker_inner_w + 2.0 * TILE_PICKER_BORDER;
                        let picker_outer_h = picker_inner_h + 2.0 * TILE_PICKER_BORDER;

                        self.picker_draw_list.clear();
                        // Border
                        self.picker_draw_list.solid_rect(
                            picker_x,
                            picker_y,
                            picker_outer_w,
                            picker_outer_h,
                            TILE_PICKER_BORDER_COLOR,
                        );
                        // Background
                        self.picker_draw_list.solid_rect(
                            picker_x + TILE_PICKER_BORDER,
                            picker_y + TILE_PICKER_BORDER,
                            picker_inner_w,
                            picker_inner_h,
                            TILE_PICKER_BG_COLOR,
                        );

                        // Highlight selected tile
                        if let Some(active_id) = self.active_voxel_type
                            && let Some(idx) =
                                TILE_PICKER_TILE_IDS.iter().position(|&id| id == active_id)
                        {
                            let highlight_pad = 3.0;
                            let hx = picker_x
                                + TILE_PICKER_BORDER
                                + TILE_PICKER_PADDING
                                + idx as f32 * (TILE_PICKER_TILE_SIZE + TILE_PICKER_SPACING)
                                - highlight_pad;
                            let hy =
                                picker_y + TILE_PICKER_BORDER + TILE_PICKER_PADDING - highlight_pad;
                            self.picker_draw_list.solid_rect(
                                hx,
                                hy,
                                TILE_PICKER_TILE_SIZE + 2.0 * highlight_pad,
                                TILE_PICKER_TILE_SIZE + 2.0 * highlight_pad,
                                TILE_PICKER_HIGHLIGHT_COLOR,
                            );
                        }

                        // Torch slot position (after the 6 block tiles + separator)
                        let torch_slot_x = picker_x
                            + TILE_PICKER_BORDER
                            + TILE_PICKER_PADDING
                            + num_tiles * (TILE_PICKER_TILE_SIZE + TILE_PICKER_SPACING)
                            - TILE_PICKER_SPACING
                            + torch_separator;
                        let torch_slot_y = picker_y + TILE_PICKER_BORDER + TILE_PICKER_PADDING;

                        // Highlight torch slot if selected
                        if self.active_voxel_type == Some(mesh_catalog::MESH_TORCH) {
                            let highlight_pad = 3.0;
                            self.picker_draw_list.solid_rect(
                                torch_slot_x - highlight_pad,
                                torch_slot_y - highlight_pad,
                                TILE_PICKER_TILE_SIZE + 2.0 * highlight_pad,
                                TILE_PICKER_TILE_SIZE + 2.0 * highlight_pad,
                                TILE_PICKER_HIGHLIGHT_COLOR,
                            );
                        }

                        // Draw torch icon: brown stick + orange/yellow flame
                        {
                            let ts = TILE_PICKER_TILE_SIZE;
                            // Stick (brown rectangle)
                            let stick_w = ts * 0.25;
                            let stick_h = ts * 0.55;
                            let stick_x = torch_slot_x + (ts - stick_w) / 2.0;
                            let stick_y = torch_slot_y + ts - stick_h;
                            self.picker_draw_list.solid_rect(
                                stick_x,
                                stick_y,
                                stick_w,
                                stick_h,
                                Rgba::new(140, 90, 38, 255),
                            );
                            // Flame base (orange-yellow rectangle)
                            let flame_w = ts * 0.4;
                            let flame_h = ts * 0.3;
                            let flame_x = torch_slot_x + (ts - flame_w) / 2.0;
                            let flame_y = torch_slot_y + ts * 0.15;
                            self.picker_draw_list.solid_rect(
                                flame_x,
                                flame_y,
                                flame_w,
                                flame_h,
                                Rgba::new(255, 200, 50, 255),
                            );
                            // Flame tip (smaller orange rectangle)
                            let tip_w = ts * 0.2;
                            let tip_h = ts * 0.15;
                            let tip_x = torch_slot_x + (ts - tip_w) / 2.0;
                            let tip_y = torch_slot_y;
                            self.picker_draw_list.solid_rect(
                                tip_x,
                                tip_y,
                                tip_w,
                                tip_h,
                                Rgba::new(255, 115, 13, 255),
                            );
                        }

                        if let Some(solid_bg) = &self.solid_bind_group {
                            renderer.render_overlay(
                                &view,
                                &mut encoder,
                                &self.picker_draw_list,
                                solid_bg,
                            );
                        }

                        // Tile texture previews
                        self.hud_draw_list.clear();
                        for (i, &tile_id) in TILE_PICKER_TILE_IDS.iter().enumerate() {
                            let uv = self.voxel_uv_map[tile_id as usize];
                            if uv[2] > uv[0] {
                                let tx = picker_x
                                    + TILE_PICKER_BORDER
                                    + TILE_PICKER_PADDING
                                    + i as f32 * (TILE_PICKER_TILE_SIZE + TILE_PICKER_SPACING);
                                let ty = picker_y + TILE_PICKER_BORDER + TILE_PICKER_PADDING;
                                self.hud_draw_list.rect(
                                    tx,
                                    ty,
                                    TILE_PICKER_TILE_SIZE,
                                    TILE_PICKER_TILE_SIZE,
                                    Vec2::new(uv[0], uv[1]),
                                    Vec2::new(uv[2], uv[3]),
                                    Rgba::WHITE,
                                );
                            }
                        }
                        if let Some(voxel_bg) = &self.voxel_atlas_overlay_bind_group {
                            renderer.render_overlay(
                                &view,
                                &mut encoder,
                                &self.hud_draw_list,
                                voxel_bg,
                            );
                        }

                        // Key labels below tiles
                        if let Some(font) = &self.font {
                            self.draw_list.clear();
                            for (i, &tile_id) in TILE_PICKER_TILE_IDS.iter().enumerate() {
                                let tx = picker_x
                                    + TILE_PICKER_BORDER
                                    + TILE_PICKER_PADDING
                                    + i as f32 * (TILE_PICKER_TILE_SIZE + TILE_PICKER_SPACING);
                                let ty = picker_y + TILE_PICKER_BORDER + TILE_PICKER_PADDING;
                                let label = format!("{}", i + 1);
                                let char_width = font.text_width(&label);
                                let label_x = tx + TILE_PICKER_TILE_SIZE / 2.0 - char_width / 2.0;
                                let label_y = ty + TILE_PICKER_TILE_SIZE + 4.0;
                                let label_color = if self.active_voxel_type == Some(tile_id) {
                                    TILE_PICKER_HIGHLIGHT_COLOR
                                } else {
                                    Rgba::WHITE
                                };
                                font.draw_text(
                                    &mut self.draw_list,
                                    &label,
                                    label_x,
                                    label_y,
                                    label_color,
                                );
                            }
                            // Torch "9" label
                            {
                                let label = "9";
                                let char_width = font.text_width(label);
                                let label_x = torch_slot_x + TILE_PICKER_TILE_SIZE / 2.0 - char_width / 2.0;
                                let label_y = torch_slot_y + TILE_PICKER_TILE_SIZE + 4.0;
                                let label_color = if self.active_voxel_type == Some(mesh_catalog::MESH_TORCH) {
                                    TILE_PICKER_HIGHLIGHT_COLOR
                                } else {
                                    Rgba::WHITE
                                };
                                font.draw_text(
                                    &mut self.draw_list,
                                    label,
                                    label_x,
                                    label_y,
                                    label_color,
                                );
                            }

                            // "0: none" hint
                            let hint_x = picker_x + picker_outer_w + 6.0;
                            let hint_y = picker_y + picker_outer_h - 18.0;
                            font.draw_text(
                                &mut self.draw_list,
                                "0: none",
                                hint_x,
                                hint_y,
                                Rgba::new(180, 180, 180, 200),
                            );

                            if let Some(font_bg) = &self.font_bind_group {
                                renderer.render_overlay(
                                    &view,
                                    &mut encoder,
                                    &self.draw_list,
                                    font_bg,
                                );
                            }
                        }

                        renderer.submit(encoder);
                        frame.present();
                    }
                    window.request_redraw();
                }
            }
            _ => {}
        }
    }
}

fn generate_crosshair_texture() -> Texture {
    let size = 32u32;
    let mut tex = Texture::new(size, size);
    let center = size / 2;
    let arm_length = 6u32;
    let arm_half_width = 1u32;

    for i in (center - arm_length)..=(center + arm_length) {
        for w in 0..=(arm_half_width * 2) {
            let offset = center - arm_half_width + w;
            // Horizontal arm
            tex.set_pixel(i, offset, Rgba::new(255, 255, 255, 153));
            // Vertical arm
            tex.set_pixel(offset, i, Rgba::new(255, 255, 255, 153));
        }
    }

    // Center 2x2 brighter
    for dy in 0..2u32 {
        for dx in 0..2u32 {
            tex.set_pixel(
                center - 1 + dx,
                center - 1 + dy,
                Rgba::new(255, 255, 255, 230),
            );
        }
    }

    tex
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn setup_tracing(log_dir: &std::path::Path) -> tracing_appender::non_blocking::WorkerGuard {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let file_appender = tracing_appender::rolling::daily(log_dir, "voxels.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let default_filter = "warn,raytracing_voxels=trace";
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| default_filter.into());

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    guard
}

fn main() -> Result<()> {
    let config = Config::load()?;
    let _log_guard = setup_tracing(&config.log_dir);

    info!(log_dir = %config.log_dir.display(), "logging initialized");
    info!(storage_dir = %config.chunk_storage_dir.display(), "chunk storage directory");

    let seed = seed::resolve_seed(&config.chunk_storage_dir, config.seed)?;
    info!(seed, "world seed resolved");

    let event_loop = EventLoop::new().context("failed to create event loop")?;
    let mut app = App::new(config.chunk_storage_dir.clone(), seed, &config);

    info!("entering event loop");
    event_loop.run_app(&mut app).context("event loop error")?;
    Ok(())
}
