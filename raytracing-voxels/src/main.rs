mod camera;
mod chunk;
mod chunk_manager;
mod config;
mod overlay;
mod overlay_renderer;
mod player;
mod texture_atlas;
mod texture_atlas_font;
mod voxel_renderer;
mod voxel_textures;
mod terrain;
mod world;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use glam::{IVec3, Vec2, Vec3};

use camera::Camera;
use chunk_manager::ChunkManager;
use config::Config;
use overlay::{DrawList, Rgba, Texture};
use player::{InputState, Player};
use texture_atlas_font::TextureAtlasFont;
use voxel_renderer::Renderer;
use world::World;

const VOXEL_TYPE_NAMES: &[&str] = &["", "stone", "dirt", "grass", "brick"];
const VOXEL_KEY_TO_ID: [u8; 5] = [0, 3, 2, 1, 4];

const MINECRAFT_FONT: &[u8] = include_bytes!("../resources/Minecraft.otf");
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};
use winit::window::{CursorGrabMode, Window, WindowId};

const MOUSE_SENSITIVITY: f32 = 0.003;
const MAX_DT: f32 = 0.1;

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
    active_voxel_type: u8,
    crosshair_bind_group: Option<wgpu::BindGroup>,
    voxel_atlas_overlay_bind_group: Option<wgpu::BindGroup>,
    voxel_uv_map: [[f32; 4]; 256],
    last_save: Instant,
    storage_dir: PathBuf,
}

impl App {
    fn new(storage_dir: PathBuf, seed: u32) -> Self {
        let mut world = World::new();
        let mut chunk_manager = ChunkManager::new(7, 5000, 5.0, storage_dir.clone(), seed);

        let camera = match Camera::load_from_file(&camera::camera_file_path(&storage_dir)) {
            Ok(cam) => {
                log::info!("loaded camera from {}", storage_dir.display());
                cam
            }
            Err(_) => {
                let cam_pos = Vec3::new(24.0, 64.0, 40.0);
                let delta = Vec3::ZERO - cam_pos;
                let horizontal_dist = Vec3::new(delta.x, 0.0, delta.z).length();
                Camera::new(
                    cam_pos,
                    f32::atan2(-delta.x, -delta.z),
                    f32::atan2(delta.y, horizontal_dist),
                    60.0_f32.to_radians(),
                )
            }
        };

        chunk_manager.load_initial(camera.position, &mut world);

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
            active_voxel_type: VOXEL_KEY_TO_ID[1], // default: grass
            crosshair_bind_group: None,
            voxel_atlas_overlay_bind_group: None,
            voxel_uv_map: [[0.0; 4]; 256],
            last_save: Instant::now(),
            storage_dir,
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
        }
    }
}

impl App {
    fn try_resume(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let window_attrs = Window::default_attributes().with_title("Raytracing Voxels");
        let window = Arc::new(
            event_loop
                .create_window(window_attrs)
                .context("failed to create window")?,
        );
        let mut renderer = Renderer::new(window.clone())?;
        let (voxel_data, chunk_infos) = self.world.pack_gpu_data();
        renderer.upload_world(&voxel_data, &chunk_infos, chunk_infos.len() as u32);
        self.world.clear_dirty();

        let atlas = voxel_textures::build_voxel_atlas(42)?;
        renderer.upload_voxel_atlas(atlas.texture(), atlas.uv_map());
        self.voxel_uv_map = *atlas.uv_map();

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

        self.window = Some(window);
        self.renderer = Some(renderer);
        self.last_frame = Instant::now();
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        if let Err(e) = self.try_resume(event_loop) {
            log::error!("failed to initialize: {e:#}");
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
                let cam_path = camera::camera_file_path(&self.storage_dir);
                if let Err(e) = self.camera.save_to_file(&cam_path) {
                    log::error!("failed to save camera: {e:#}");
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
                    if let Some(hit) =
                        self.world
                            .raycast(self.camera.position, self.camera.forward(), 50.0)
                    {
                        let hit_world = hit.chunk_pos * 16
                            + IVec3::new(
                                hit.local_pos[0] as i32,
                                hit.local_pos[1] as i32,
                                hit.local_pos[2] as i32,
                            );
                        let place_pos = hit_world + hit.normal;
                        self.world.set_voxel(place_pos, self.active_voxel_type);
                    }
                } else {
                    self.grab_cursor();
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                ..
            } => {
                if self.cursor_grabbed
                    && let Some(hit) =
                        self.world
                            .raycast(self.camera.position, self.camera.forward(), 50.0)
                {
                    let hit_world = hit.chunk_pos * 16
                        + IVec3::new(
                            hit.local_pos[0] as i32,
                            hit.local_pos[1] as i32,
                            hit.local_pos[2] as i32,
                        );
                    self.world.set_voxel(hit_world, 0);
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
                    event_loop.exit();
                }
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(code),
                        state,
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
                    KeyCode::Space => self.input.up = pressed,
                    KeyCode::ShiftLeft => self.input.down = pressed,
                    KeyCode::F1 if pressed => self.player.toggle_fly_mode(),
                    KeyCode::Digit1 if pressed => self.active_voxel_type = VOXEL_KEY_TO_ID[1],
                    KeyCode::Digit2 if pressed => self.active_voxel_type = VOXEL_KEY_TO_ID[2],
                    KeyCode::Digit3 if pressed => self.active_voxel_type = VOXEL_KEY_TO_ID[3],
                    KeyCode::Digit4 if pressed => self.active_voxel_type = VOXEL_KEY_TO_ID[4],
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

                self.player.tick(dt, &self.input, self.camera.yaw, &self.world);
                self.camera.position = self.player.eye_position();

                self.chunk_manager.update_camera(self.camera.position);
                self.chunk_manager.drain_results(&mut self.world);

                if self.last_save.elapsed().as_secs_f32() >= 5.0 {
                    self.chunk_manager.save_modified(&mut self.world);
                    let cam_path = camera::camera_file_path(&self.storage_dir);
                    if let Err(e) = self.camera.save_to_file(&cam_path) {
                        log::error!("failed to save camera: {e:#}");
                    }
                    self.last_save = Instant::now();
                }

                if let (Some(renderer), Some(window)) = (&mut self.renderer, &self.window) {
                    if self.world.is_dirty() {
                        let (voxel_data, chunk_infos) = self.world.pack_gpu_data();
                        renderer.upload_world(&voxel_data, &chunk_infos, chunk_infos.len() as u32);
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
                            font.draw_text(
                                &mut self.draw_list,
                                &format!("CPU: chunks {}", format_bytes(cpu_chunk_bytes)),
                                10.0,
                                y,
                                Rgba::WHITE,
                            );
                            y += line_height;

                            let gpu_stats = renderer.gpu_memory_stats();
                            let gpu_chunk_total = gpu_stats.voxel_buffer_bytes
                                + gpu_stats.chunk_info_bytes
                                + gpu_stats.chunk_count_bytes;

                            let font_atlas_bytes = self.font.as_ref().map_or(0u64, |f| {
                                let tex = f.atlas().texture();
                                tex.width() as u64 * tex.height() as u64 * 4
                            });
                            let crosshair_bytes: u64 = 32 * 32 * 4;

                            font.draw_text(
                                &mut self.draw_list,
                                &format!(
                                    "GPU: chunks {} | atlas {} | font {} | xhair {}",
                                    format_bytes(gpu_chunk_total),
                                    format_bytes(gpu_stats.voxel_atlas_bytes),
                                    format_bytes(font_atlas_bytes),
                                    format_bytes(crosshair_bytes),
                                ),
                                10.0,
                                y,
                                Rgba::WHITE,
                            );
                            y += line_height;

                            let cpu_font_bytes = font_atlas_bytes;
                            let total_cpu = cpu_chunk_bytes + cpu_font_bytes;
                            let total_gpu = gpu_chunk_total
                                + gpu_stats.voxel_atlas_bytes
                                + font_atlas_bytes
                                + crosshair_bytes;
                            font.draw_text(
                                &mut self.draw_list,
                                &format!(
                                    "Total: CPU {} | GPU {}",
                                    format_bytes(total_cpu),
                                    format_bytes(total_gpu),
                                ),
                                10.0,
                                y,
                                Rgba::WHITE,
                            );
                            y += line_height;

                            // Active voxel HUD
                            let voxel_name = VOXEL_TYPE_NAMES[self.active_voxel_type as usize];
                            font.draw_text(
                                &mut self.draw_list,
                                &format!("Active: {}", voxel_name),
                                10.0,
                                screen_height - line_height - 50.0,
                                Rgba::WHITE,
                            );

                            let mode_text = if self.player.fly_mode {
                                "Mode: FLY"
                            } else {
                                "Mode: WALK"
                            };
                            font.draw_text(
                                &mut self.draw_list,
                                mode_text,
                                10.0,
                                screen_height - 2.0 * line_height - 50.0,
                                Rgba::WHITE,
                            );
                            let _ = y;
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

                        // Voxel preview HUD
                        self.hud_draw_list.clear();
                        let uv = self.voxel_uv_map[self.active_voxel_type as usize];
                        if uv[2] > uv[0] {
                            let preview_size = 48.0;
                            self.hud_draw_list.rect(
                                10.0,
                                screen_height - preview_size - 10.0,
                                preview_size,
                                preview_size,
                                Vec2::new(uv[0], uv[1]),
                                Vec2::new(uv[2], uv[3]),
                                Rgba::WHITE,
                            );
                        }
                        if let Some(voxel_bg) = &self.voxel_atlas_overlay_bind_group {
                            renderer.render_overlay(
                                &view,
                                &mut encoder,
                                &self.hud_draw_list,
                                voxel_bg,
                            );
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

fn main() -> Result<()> {
    env_logger::init();
    let config = Config::load()?;
    let event_loop = EventLoop::new().context("failed to create event loop")?;
    let mut app = App::new(config.chunk_storage_dir, config.seed);
    event_loop.run_app(&mut app).context("event loop error")?;
    Ok(())
}
