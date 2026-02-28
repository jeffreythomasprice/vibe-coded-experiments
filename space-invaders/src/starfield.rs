use anyhow::Result;
use glam::{Mat3, Vec2, Vec4};
use rand::{Rng, SeedableRng};

use crate::gfx::{Mesh, Vertex2d};
use crate::renderer::{Actor, ActorId, Renderer};
use crate::resources::TextureAtlas;

struct Star {
    actor_id: ActorId,
    pos: Vec2,
    speed: f32,
}

pub struct StarField {
    stars: Vec<Star>,
    game_area: Vec2,
    rng: rand::rngs::SmallRng,
}

fn make_star_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &TextureAtlas,
    scale: f32,
) -> Result<Mesh<Vertex2d>> {
    let uv = atlas
        .get("star")
        .ok_or_else(|| anyhow::anyhow!("sprite 'star' not in atlas"))?;
    let (pw, ph) = atlas
        .size("star")
        .ok_or_else(|| anyhow::anyhow!("sprite 'star' size not in atlas"))?;
    let hw = pw as f32 * scale * 0.5;
    let hh = ph as f32 * scale * 0.5;
    let mut mesh = Mesh::new(device, 4, 6);
    let color = Vec4::new(1.0, 1.0, 1.0, 0.1);
    mesh.append_quad(
        queue,
        Vertex2d {
            position: Vec2::new(-hw, -hh),
            uv: Vec2::new(uv.min.x, uv.min.y),
            color,
        },
        Vertex2d {
            position: Vec2::new(hw, -hh),
            uv: Vec2::new(uv.max.x, uv.min.y),
            color,
        },
        Vertex2d {
            position: Vec2::new(hw, hh),
            uv: Vec2::new(uv.max.x, uv.max.y),
            color,
        },
        Vertex2d {
            position: Vec2::new(-hw, hh),
            uv: Vec2::new(uv.min.x, uv.max.y),
            color,
        },
    );
    Ok(mesh)
}

impl StarField {
    pub fn new(
        renderer: &mut Renderer,
        atlas: &TextureAtlas,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        game_area: Vec2,
        rng: &mut impl Rng,
    ) -> Result<Self> {
        let mut stars = Vec::new();
        let mut inner_rng = rand::rngs::SmallRng::seed_from_u64(rng.random());

        for _ in 0..30 {
            let pos = Vec2::new(
                inner_rng.random_range(0.0..game_area.x),
                inner_rng.random_range(0.0..game_area.y),
            );
            let mesh = make_star_mesh(device, queue, atlas, 1.0)?;
            let actor_id = renderer.add_actor(Actor {
                mesh,
                texture: atlas.texture_arc(),
                transform: Mat3::from_translation(pos),
                z_index: -2,
            });
            stars.push(Star {
                actor_id,
                pos,
                speed: 25.0,
            });
        }

        for _ in 0..15 {
            let pos = Vec2::new(
                inner_rng.random_range(0.0..game_area.x),
                inner_rng.random_range(0.0..game_area.y),
            );
            let mesh = make_star_mesh(device, queue, atlas, 2.0)?;
            let actor_id = renderer.add_actor(Actor {
                mesh,
                texture: atlas.texture_arc(),
                transform: Mat3::from_translation(pos),
                z_index: -1,
            });
            stars.push(Star {
                actor_id,
                pos,
                speed: 60.0,
            });
        }

        Ok(Self {
            stars,
            game_area,
            rng: inner_rng,
        })
    }

    pub fn update(&mut self, dt: f32, renderer: &mut Renderer) {
        let dt = dt.min(0.1);
        let margin = 16.0;
        for star in &mut self.stars {
            star.pos.y += star.speed * dt;
            if star.pos.y > self.game_area.y + margin {
                star.pos.y = -margin;
                star.pos.x = self.rng.random_range(0.0..self.game_area.x);
            }
            if let Some(actor) = renderer.actor_mut(star.actor_id) {
                actor.transform = Mat3::from_translation(star.pos);
            }
        }
    }
}
