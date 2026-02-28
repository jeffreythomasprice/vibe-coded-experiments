use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use glam::{Mat3, Mat4, Vec4};

use crate::gfx::{Mesh, Pipeline2dTextured, Texture, Vertex2d};
use crate::state_machine::GfxState;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Uniforms {
    projection: Mat4,
    modelview: Mat4,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActorId(u64);

pub struct Actor {
    pub mesh: Mesh<Vertex2d>,
    pub texture: Arc<Texture>,
    pub transform: Mat3,
    pub z_index: i32,
}

pub struct Renderer {
    shader: wgpu::ShaderModule,
    pipeline: Pipeline2dTextured<Uniforms>,
    format: wgpu::TextureFormat,
    capacity: usize,
    actors: HashMap<ActorId, Actor>,
    sorted_order: Vec<ActorId>,
    sort_dirty: bool,
    next_id: u64,
}

impl Renderer {
    pub fn new(gfx: &GfxState) -> Result<Self> {
        let shader = gfx.device.create_shader_module(wgpu::include_wgsl!("shader2d.wgsl"));
        let capacity = 16;
        let pipeline = Pipeline2dTextured::new(
            &gfx.device,
            &shader,
            "vs_main",
            "fs_main",
            gfx.config.format,
            capacity,
        )?;
        let format = gfx.config.format;
        Ok(Self {
            shader,
            pipeline,
            format,
            capacity,
            actors: HashMap::new(),
            sorted_order: Vec::new(),
            sort_dirty: false,
            next_id: 0,
        })
    }

    pub fn add_actor(&mut self, actor: Actor) -> ActorId {
        let id = ActorId(self.next_id);
        self.next_id += 1;
        self.actors.insert(id, actor);
        self.sorted_order.push(id);
        self.sort_dirty = true;
        id
    }

    pub fn remove_actor(&mut self, id: ActorId) {
        self.actors.remove(&id);
        self.sorted_order.retain(|&x| x != id);
    }

    pub fn actor(&self, id: ActorId) -> Option<&Actor> {
        self.actors.get(&id)
    }

    pub fn actor_mut(&mut self, id: ActorId) -> Option<&mut Actor> {
        self.sort_dirty = true;
        self.actors.get_mut(&id)
    }

    pub fn draw(
        &mut self,
        gfx: &GfxState,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        clear_color: Option<wgpu::Color>,
    ) -> Result<()> {
        if self.actors.len() > self.capacity {
            let new_capacity = self.actors.len().next_power_of_two();
            self.pipeline = Pipeline2dTextured::new(
                &gfx.device,
                &self.shader,
                "vs_main",
                "fs_main",
                self.format,
                new_capacity,
            )?;
            self.capacity = new_capacity;
        }

        if self.sort_dirty {
            let actors = &self.actors;
            self.sorted_order.sort_by_key(|id| {
                let a = &actors[id];
                (a.z_index, Arc::as_ptr(&a.texture) as usize)
            });
            self.sort_dirty = false;
        }

        let win_w = gfx.config.width as f32;
        let win_h = gfx.config.height as f32;
        let scale = (win_w / 640.0).min(win_h / 480.0);
        let vp_w = 640.0 * scale;
        let vp_h = 480.0 * scale;
        let vp_x = (win_w - vp_w) / 2.0;
        let vp_y = (win_h - vp_h) / 2.0;

        let projection = Mat4::orthographic_rh(0.0, 640.0, 480.0, 0.0, -1.0, 1.0);

        let mut pass = self.pipeline.begin_render_pass(&gfx.device, encoder, view, clear_color);
        pass.set_viewport(vp_x, vp_y, vp_w, vp_h);

        for id in &self.sorted_order {
            if let Some(actor) = self.actors.get(id) {
                let modelview = mat3_to_mat4(actor.transform);
                pass.set_draw_state(&gfx.queue, Uniforms { projection, modelview }, &actor.texture);
                pass.draw(&actor.mesh, 0..actor.mesh.index_size() as u32);
            }
        }

        Ok(())
    }
}

fn mat3_to_mat4(m: Mat3) -> Mat4 {
    Mat4::from_cols(
        Vec4::new(m.x_axis.x, m.x_axis.y, 0.0, 0.0),
        Vec4::new(m.y_axis.x, m.y_axis.y, 0.0, 0.0),
        Vec4::new(0.0, 0.0, 1.0, 0.0),
        Vec4::new(m.z_axis.x, m.z_axis.y, 0.0, 1.0),
    )
}
