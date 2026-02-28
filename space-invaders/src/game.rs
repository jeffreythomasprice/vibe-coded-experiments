use std::collections::{HashMap, HashSet};

use anyhow::Result;
use glam::{Mat3, Vec2, Vec4};
use rand::{Rng, SeedableRng};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::keyboard::{Key, NamedKey};

use crate::gfx::{Mesh, Vertex2d};
use crate::menu::Menu;
use crate::physics::{
    ColliderShape, PhysicsActorDesc, PhysicsActorId, PhysicsBodyType, PhysicsWorld,
};
use crate::renderer::{Actor, ActorId, Renderer};
use crate::resources::{Text, TextureAtlas};
use crate::starfield::StarField;
use crate::state_machine::{GfxState, State, StateSwitch};

pub const GAME_WIDTH: u32 = 640;
pub const GAME_HEIGHT: u32 = 480;

const GROUP_PLAYER: u32 = 0b0001;
const GROUP_ENEMY: u32 = 0b0010;
const GROUP_PLAYER_BULLET: u32 = 0b0100;
const GROUP_ENEMY_BULLET: u32 = 0b1000;

const PLAYER_SPEED: f32 = 250.0;
const PLAYER_SCALE: f32 = 4.0;
const ENEMY_SCALE: f32 = 3.0;
const PROJ_SCALE: f32 = 2.0;
const PLAYER_BULLET_SPEED: f32 = 800.0;
const ENEMY_BULLET_SPEED: f32 = 700.0;
const FIRE_COOLDOWN: f32 = 0.25;
const ENEMY_FIRE_INTERVAL: f32 = 2.0;
const FORMATION_BASE_INTERVAL: f32 = 0.8;
const FORMATION_STEP_SIZE: f32 = 12.0;
const FORMATION_DROP: f32 = 24.0;
const FORMATION_MARGIN: f32 = 24.0;
const ANIM_INTERVAL: f32 = 0.5;
const GRID_COLS: usize = 5;
const GRID_ROWS: usize = 3;

#[derive(Clone, Copy)]
enum EntityKind {
    Player,
    Enemy,
    PlayerProjectile,
    EnemyProjectile,
}

struct BoundActor {
    actor_id: ActorId,
    physics_id: PhysicsActorId,
    pos: Vec2,
}

impl BoundActor {
    fn despawn(
        self,
        renderer: &mut Renderer,
        physics: &mut PhysicsWorld,
        entity_map: &mut HashMap<PhysicsActorId, EntityKind>,
    ) {
        renderer.remove_actor(self.actor_id);
        physics.remove_actor(self.physics_id);
        entity_map.remove(&self.physics_id);
    }
}

struct PlayerEntity {
    bound: BoundActor,
    lives: u32,
    fire_cooldown: f32,
}

#[derive(Clone, Copy)]
enum EnemyKind {
    Squid,
    Crab,
    Octopus,
}

struct EnemyEntity {
    bound: BoundActor,
    kind: EnemyKind,
    anim_timer: f32,
    anim_frame: u8,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProjectileOwner {
    Player,
    Enemy,
}

struct ProjectileEntity {
    bound: BoundActor,
}

struct FormationState {
    direction: f32,
    drop_pending: bool,
    step_timer: f32,
    step_interval: f32,
}

struct InputState {
    left: bool,
    right: bool,
    fire: bool,
}

struct SpawnCtx<'a> {
    renderer: &'a mut Renderer,
    physics: &'a mut PhysicsWorld,
    atlas: &'a TextureAtlas,
    sprite_hulls: &'a HashMap<String, Vec<Vec2>>,
    entity_map: &'a mut HashMap<PhysicsActorId, EntityKind>,
    device: &'a wgpu::Device,
    queue: &'a wgpu::Queue,
}

// Spawn requests queued in update(), executed in render() where &GfxState is available.
enum PendingSpawn {
    Projectile {
        pos: Vec2,
        vel: Vec2,
        owner: ProjectileOwner,
    },
    Formation,
}

pub struct GameState {
    renderer: Renderer,
    physics: PhysicsWorld,
    atlas: TextureAtlas,
    sprite_hulls: HashMap<String, Vec<Vec2>>,
    game_area: Vec2,

    player: Option<PlayerEntity>,
    enemies: Vec<EnemyEntity>,
    projectiles: Vec<ProjectileEntity>,
    entity_map: HashMap<PhysicsActorId, EntityKind>,

    formation: FormationState,
    input: InputState,
    enemy_fire_timer: f32,
    rng: rand::rngs::SmallRng,
    score: u32,
    game_over: bool,

    pending_spawns: Vec<PendingSpawn>,
    starfield: StarField,

    lives_text: Text,
    lives_mesh: Mesh<Vertex2d>,
    lives_actor_id: ActorId,
    last_lives: u32,

    paused: bool,
    pause_menu: Menu,
    game_over_menu: Menu,
    pending_restart: bool,
}

fn enemy_sprite_name(kind: EnemyKind, frame: u8) -> &'static str {
    match (kind, frame % 2) {
        (EnemyKind::Squid, 0) => "squid_f1",
        (EnemyKind::Squid, _) => "squid_f2",
        (EnemyKind::Crab, 0) => "crab_f1",
        (EnemyKind::Crab, _) => "crab_f2",
        (EnemyKind::Octopus, 0) => "octopus_f1",
        (EnemyKind::Octopus, _) => "octopus_f2",
    }
}

fn make_hull(hulls: &HashMap<String, Vec<Vec2>>, names: &[&str], scale: f32) -> ColliderShape {
    let points: Vec<Vec2> = names
        .iter()
        .filter_map(|n| hulls.get(*n))
        .flat_map(|pts| pts.iter().map(|p| *p * scale))
        .collect();
    ColliderShape::ConvexHull { points }
}

fn make_quad_mesh(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    atlas: &TextureAtlas,
    sprite_name: &str,
    scale: f32,
) -> Result<Mesh<Vertex2d>> {
    let uv = atlas
        .get(sprite_name)
        .ok_or_else(|| anyhow::anyhow!("sprite '{}' not in atlas", sprite_name))?;
    let (pw, ph) = atlas
        .size(sprite_name)
        .ok_or_else(|| anyhow::anyhow!("sprite '{}' size not in atlas", sprite_name))?;
    let hw = pw as f32 * scale * 0.5;
    let hh = ph as f32 * scale * 0.5;
    let mut mesh = Mesh::new(device, 4, 6);
    mesh.append_quad(
        queue,
        Vertex2d {
            position: Vec2::new(-hw, -hh),
            uv: Vec2::new(uv.min.x, uv.min.y),
            color: Vec4::ONE,
        },
        Vertex2d {
            position: Vec2::new(hw, -hh),
            uv: Vec2::new(uv.max.x, uv.min.y),
            color: Vec4::ONE,
        },
        Vertex2d {
            position: Vec2::new(hw, hh),
            uv: Vec2::new(uv.max.x, uv.max.y),
            color: Vec4::ONE,
        },
        Vertex2d {
            position: Vec2::new(-hw, hh),
            uv: Vec2::new(uv.min.x, uv.max.y),
            color: Vec4::ONE,
        },
    );
    Ok(mesh)
}

fn spawn_player(
    renderer: &mut Renderer,
    physics: &mut PhysicsWorld,
    atlas: &TextureAtlas,
    sprite_hulls: &HashMap<String, Vec<Vec2>>,
    entity_map: &mut HashMap<PhysicsActorId, EntityKind>,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    game_area: Vec2,
) -> Result<PlayerEntity> {
    let pos = Vec2::new(game_area.x * 0.5, game_area.y - 40.0);
    let mesh = make_quad_mesh(device, queue, atlas, "ship1", PLAYER_SCALE)?;
    let actor_id = renderer.add_actor(Actor {
        mesh,
        texture: atlas.texture_arc(),
        transform: Mat3::from_translation(pos),
        z_index: 1,
    });
    let physics_id = physics.add_actor(PhysicsActorDesc {
        position: pos,
        rotation: 0.0,
        velocity: Vec2::ZERO,
        angular_velocity: 0.0,
        shape: make_hull(sprite_hulls, &["ship1"], PLAYER_SCALE),
        body_type: PhysicsBodyType::Kinematic,
        restitution: 0.0,
        interaction_groups: Some((GROUP_PLAYER, GROUP_ENEMY_BULLET)),
    });
    entity_map.insert(physics_id, EntityKind::Player);
    Ok(PlayerEntity {
        bound: BoundActor {
            actor_id,
            physics_id,
            pos,
        },
        lives: 3,
        fire_cooldown: 0.0,
    })
}

fn spawn_formation(ctx: &mut SpawnCtx<'_>, game_area: Vec2) -> Result<Vec<EnemyEntity>> {
    let mut enemies = Vec::new();
    let cell_w = 48.0f32;
    let cell_h = 40.0f32;
    let origin_x = game_area.x * 0.15;
    let origin_y = game_area.y * 0.08;

    for row in 0..GRID_ROWS {
        let kind = match row {
            0 => EnemyKind::Squid,
            1 => EnemyKind::Crab,
            _ => EnemyKind::Octopus,
        };
        let sprite_name = enemy_sprite_name(kind, 0);
        let hull_names: &[&str] = match kind {
            EnemyKind::Squid => &["squid_f1", "squid_f2"],
            EnemyKind::Crab => &["crab_f1", "crab_f2"],
            EnemyKind::Octopus => &["octopus_f1", "octopus_f2"],
        };
        for col in 0..GRID_COLS {
            let pos = Vec2::new(
                origin_x + col as f32 * cell_w,
                origin_y + row as f32 * cell_h,
            );
            let mesh = make_quad_mesh(ctx.device, ctx.queue, ctx.atlas, sprite_name, ENEMY_SCALE)?;
            let actor_id = ctx.renderer.add_actor(Actor {
                mesh,
                texture: ctx.atlas.texture_arc(),
                transform: Mat3::from_translation(pos),
                z_index: 0,
            });
            let physics_id = ctx.physics.add_actor(PhysicsActorDesc {
                position: pos,
                rotation: 0.0,
                velocity: Vec2::ZERO,
                angular_velocity: 0.0,
                shape: make_hull(ctx.sprite_hulls, hull_names, ENEMY_SCALE),
                body_type: PhysicsBodyType::Kinematic,
                restitution: 0.0,
                interaction_groups: Some((GROUP_ENEMY, GROUP_PLAYER_BULLET)),
            });
            ctx.entity_map.insert(physics_id, EntityKind::Enemy);
            enemies.push(EnemyEntity {
                bound: BoundActor {
                    actor_id,
                    physics_id,
                    pos,
                },
                kind,
                anim_timer: 0.0,
                anim_frame: 0,
            });
        }
    }
    Ok(enemies)
}

fn spawn_projectile(
    ctx: &mut SpawnCtx<'_>,
    pos: Vec2,
    vel: Vec2,
    owner: ProjectileOwner,
) -> Result<ProjectileEntity> {
    let sprite_name = match owner {
        ProjectileOwner::Player => "laser_bolt",
        ProjectileOwner::Enemy => "enemy_bomb",
    };
    let mesh = make_quad_mesh(ctx.device, ctx.queue, ctx.atlas, sprite_name, PROJ_SCALE)?;
    let actor_id = ctx.renderer.add_actor(Actor {
        mesh,
        texture: ctx.atlas.texture_arc(),
        transform: Mat3::from_translation(pos),
        z_index: 0,
    });
    let groups = match owner {
        ProjectileOwner::Player => (GROUP_PLAYER_BULLET, GROUP_ENEMY | GROUP_ENEMY_BULLET),
        ProjectileOwner::Enemy => (GROUP_ENEMY_BULLET, GROUP_PLAYER | GROUP_PLAYER_BULLET),
    };
    let physics_id = ctx.physics.add_actor(PhysicsActorDesc {
        position: pos,
        rotation: 0.0,
        velocity: vel,
        angular_velocity: 0.0,
        shape: make_hull(ctx.sprite_hulls, &[sprite_name], PROJ_SCALE),
        body_type: PhysicsBodyType::Dynamic,
        restitution: 0.0,
        interaction_groups: Some(groups),
    });
    let kind = match owner {
        ProjectileOwner::Player => EntityKind::PlayerProjectile,
        ProjectileOwner::Enemy => EntityKind::EnemyProjectile,
    };
    ctx.entity_map.insert(physics_id, kind);
    Ok(ProjectileEntity {
        bound: BoundActor {
            actor_id,
            physics_id,
            pos,
        },
    })
}

impl GameState {
    pub fn new(gfx: &GfxState) -> Result<Self> {
        let (atlas, sprite_hulls) = crate::resources::load_atlas(&gfx.device, &gfx.queue)?;
        let mut renderer = Renderer::new(gfx)?;
        let mut physics = PhysicsWorld::new();
        let mut entity_map = HashMap::new();
        let game_area = Vec2::new(GAME_WIDTH as f32, GAME_HEIGHT as f32);
        let mut rng = rand::rngs::SmallRng::seed_from_u64(rand::random());

        let starfield = StarField::new(
            &mut renderer,
            &atlas,
            &gfx.device,
            &gfx.queue,
            game_area,
            &mut rng,
        )?;

        let player = spawn_player(
            &mut renderer,
            &mut physics,
            &atlas,
            &sprite_hulls,
            &mut entity_map,
            &gfx.device,
            &gfx.queue,
            game_area,
        )?;

        let enemies = spawn_formation(
            &mut SpawnCtx {
                renderer: &mut renderer,
                physics: &mut physics,
                atlas: &atlas,
                sprite_hulls: &sprite_hulls,
                entity_map: &mut entity_map,
                device: &gfx.device,
                queue: &gfx.queue,
            },
            game_area,
        )?;

        let font = ab_glyph::FontArc::try_from_slice(include_bytes!(
            "../resources/IntelOneMono-Regular.ttf"
        ))?;
        let mut lives_text = Text::new(&gfx.device, font, 20.0);
        let mut init_mesh = Mesh::new(&gfx.device, 32, 48);
        lives_text.render_text(&gfx.device, &gfx.queue, &mut init_mesh, "Lives: 3");
        let lives_texture = lives_text.texture.clone();
        let ascent = lives_text.ascent();
        let lives_actor_id = renderer.add_actor(Actor {
            mesh: init_mesh,
            texture: lives_texture,
            transform: Mat3::from_translation(Vec2::new(10.0, 10.0 + ascent)),
            z_index: 10,
        });
        let lives_mesh = Mesh::new(&gfx.device, 32, 48);
        let pause_menu = Menu::new(gfx, &["Paused", "", "Press Escape to resume."], 40.0)?;
        let game_over_menu = Menu::new(gfx, &["GAME OVER", "", "Press any key to restart."], 40.0)?;

        Ok(Self {
            renderer,
            physics,
            atlas,
            sprite_hulls,
            game_area,
            player: Some(player),
            enemies,
            projectiles: Vec::new(),
            entity_map,
            formation: FormationState {
                direction: 1.0,
                drop_pending: false,
                step_timer: FORMATION_BASE_INTERVAL,
                step_interval: FORMATION_BASE_INTERVAL,
            },
            input: InputState {
                left: false,
                right: false,
                fire: false,
            },
            enemy_fire_timer: ENEMY_FIRE_INTERVAL,
            rng,
            score: 0,
            game_over: false,
            pending_spawns: Vec::new(),
            starfield,
            lives_text,
            lives_mesh,
            lives_actor_id,
            last_lives: 3,
            paused: false,
            pause_menu,
            game_over_menu,
            pending_restart: false,
        })
    }

    fn update_inner(&mut self, dt: f32) {
        if self.paused || self.game_over {
            return;
        }
        self.starfield.update(dt, &mut self.renderer);

        // Apply player input velocity
        if let Some(player) = &self.player {
            let mut vx = 0.0f32;
            if self.input.left {
                vx -= PLAYER_SPEED;
            }
            if self.input.right {
                vx += PLAYER_SPEED;
            }
            self.physics
                .set_velocity(player.bound.physics_id, Vec2::new(vx, 0.0));
        }

        self.physics.step(dt);

        // Sync player transform, clamp to game area
        if let Some(player) = &mut self.player {
            if let Some(pos) = self.physics.position(player.bound.physics_id) {
                player.bound.pos = pos;
            }
            let clamped_x = player
                .bound
                .pos
                .x
                .clamp(FORMATION_MARGIN, self.game_area.x - FORMATION_MARGIN);
            if clamped_x != player.bound.pos.x {
                player.bound.pos.x = clamped_x;
                self.physics
                    .set_position(player.bound.physics_id, player.bound.pos);
            }
            let pos = player.bound.pos;
            let actor_id = player.bound.actor_id;
            if let Some(actor) = self.renderer.actor_mut(actor_id) {
                actor.transform = Mat3::from_translation(pos);
            }
        }

        // Sync enemy transforms
        for i in 0..self.enemies.len() {
            let phys_id = self.enemies[i].bound.physics_id;
            let actor_id = self.enemies[i].bound.actor_id;
            if let Some(pos) = self.physics.position(phys_id) {
                self.enemies[i].bound.pos = pos;
            }
            let pos = self.enemies[i].bound.pos;
            if let Some(actor) = self.renderer.actor_mut(actor_id) {
                actor.transform = Mat3::from_translation(pos);
            }
        }

        // Sync projectile transforms
        for i in 0..self.projectiles.len() {
            let phys_id = self.projectiles[i].bound.physics_id;
            let actor_id = self.projectiles[i].bound.actor_id;
            if let Some(pos) = self.physics.position(phys_id) {
                self.projectiles[i].bound.pos = pos;
            }
            let pos = self.projectiles[i].bound.pos;
            if let Some(actor) = self.renderer.actor_mut(actor_id) {
                actor.transform = Mat3::from_translation(pos);
            }
        }

        // Tick enemy animation
        for e in &mut self.enemies {
            e.anim_timer += dt;
            if e.anim_timer >= ANIM_INTERVAL {
                e.anim_timer -= ANIM_INTERVAL;
                e.anim_frame = e.anim_frame.wrapping_add(1);
            }
        }

        self.step_formation(dt);

        // Player fire cooldown
        if let Some(player) = &mut self.player {
            player.fire_cooldown -= dt;
        }

        // Player shoots
        if self.input.fire {
            let (should_fire, player_pos) = match &self.player {
                Some(p) => (p.fire_cooldown <= 0.0, Some(p.bound.pos)),
                None => (false, None),
            };
            if should_fire && let Some(pos) = player_pos {
                self.pending_spawns.push(PendingSpawn::Projectile {
                    pos,
                    vel: Vec2::new(0.0, -PLAYER_BULLET_SPEED),
                    owner: ProjectileOwner::Player,
                });
                if let Some(player) = &mut self.player {
                    player.fire_cooldown = FIRE_COOLDOWN;
                }
            }
        }

        // Enemy fires periodically
        self.enemy_fire_timer -= dt;
        if self.enemy_fire_timer <= 0.0 && !self.enemies.is_empty() {
            self.enemy_fire_timer = ENEMY_FIRE_INTERVAL;
            let idx = self.rng.random_range(0..self.enemies.len());
            let pos = self.enemies[idx].bound.pos;
            self.pending_spawns.push(PendingSpawn::Projectile {
                pos,
                vel: Vec2::new(0.0, ENEMY_BULLET_SPEED),
                owner: ProjectileOwner::Enemy,
            });
        }

        self.cull_projectiles();
        self.process_collisions();

        // Respawn formation when all enemies are dead
        if self.enemies.is_empty() {
            self.pending_spawns.push(PendingSpawn::Formation);
        }
    }

    fn step_formation(&mut self, dt: f32) {
        self.formation.step_timer -= dt;
        if self.formation.step_timer > 0.0 || self.enemies.is_empty() {
            return;
        }
        self.formation.step_timer = self.formation.step_interval;

        let min_x = self
            .enemies
            .iter()
            .map(|e| e.bound.pos.x)
            .fold(f32::INFINITY, f32::min);
        let max_x = self
            .enemies
            .iter()
            .map(|e| e.bound.pos.x)
            .fold(f32::NEG_INFINITY, f32::max);

        if self.formation.direction > 0.0 && max_x >= self.game_area.x - FORMATION_MARGIN {
            self.formation.drop_pending = true;
            self.formation.direction = -1.0;
        } else if self.formation.direction < 0.0 && min_x <= FORMATION_MARGIN {
            self.formation.drop_pending = true;
            self.formation.direction = 1.0;
        }

        let dy = if self.formation.drop_pending {
            self.formation.drop_pending = false;
            FORMATION_DROP
        } else {
            0.0
        };
        let dx = if dy == 0.0 {
            self.formation.direction * FORMATION_STEP_SIZE
        } else {
            0.0
        };

        for i in 0..self.enemies.len() {
            let phys_id = self.enemies[i].bound.physics_id;
            let actor_id = self.enemies[i].bound.actor_id;
            let old_pos = self.enemies[i].bound.pos;
            let new_pos = Vec2::new(old_pos.x + dx, old_pos.y + dy);
            self.physics.set_position(phys_id, new_pos);
            self.enemies[i].bound.pos = new_pos;
            if let Some(actor) = self.renderer.actor_mut(actor_id) {
                actor.transform = Mat3::from_translation(new_pos);
            }
        }

        let ratio = self.enemies.len() as f32 / (GRID_COLS * GRID_ROWS) as f32;
        self.formation.step_interval =
            (FORMATION_BASE_INTERVAL * ratio).max(FORMATION_BASE_INTERVAL * 0.1);
    }

    fn cull_projectiles(&mut self) {
        let game_area = self.game_area;
        let mut i = 0;
        while i < self.projectiles.len() {
            let pos = self.projectiles[i].bound.pos;
            let oob = pos.y < -50.0
                || pos.y > game_area.y + 50.0
                || pos.x < -50.0
                || pos.x > game_area.x + 50.0;
            if oob {
                let proj = self.projectiles.swap_remove(i);
                proj.bound
                    .despawn(&mut self.renderer, &mut self.physics, &mut self.entity_map);
            } else {
                i += 1;
            }
        }
    }

    fn process_collisions(&mut self) {
        let pairs = self.physics.active_collision_pairs();
        let mut dead_proj_ids: HashSet<PhysicsActorId> = HashSet::new();
        let mut dead_enemy_ids: HashSet<PhysicsActorId> = HashSet::new();
        let mut player_hit = false;

        for (a, b) in pairs {
            let ka = self.entity_map.get(&a).copied();
            let kb = self.entity_map.get(&b).copied();
            match (ka, kb) {
                (Some(EntityKind::PlayerProjectile), Some(EntityKind::Enemy)) => {
                    dead_proj_ids.insert(a);
                    dead_enemy_ids.insert(b);
                    self.score += 10;
                }
                (Some(EntityKind::Enemy), Some(EntityKind::PlayerProjectile)) => {
                    dead_proj_ids.insert(b);
                    dead_enemy_ids.insert(a);
                    self.score += 10;
                }
                (Some(EntityKind::EnemyProjectile), Some(EntityKind::Player)) => {
                    player_hit = true;
                    dead_proj_ids.insert(a);
                }
                (Some(EntityKind::Player), Some(EntityKind::EnemyProjectile)) => {
                    player_hit = true;
                    dead_proj_ids.insert(b);
                }
                (Some(EntityKind::PlayerProjectile), Some(EntityKind::EnemyProjectile))
                | (Some(EntityKind::EnemyProjectile), Some(EntityKind::PlayerProjectile)) => {
                    dead_proj_ids.insert(a);
                    dead_proj_ids.insert(b);
                }
                _ => {}
            }
        }

        let mut i = 0;
        while i < self.enemies.len() {
            if dead_enemy_ids.contains(&self.enemies[i].bound.physics_id) {
                let e = self.enemies.swap_remove(i);
                e.bound
                    .despawn(&mut self.renderer, &mut self.physics, &mut self.entity_map);
            } else {
                i += 1;
            }
        }

        let mut i = 0;
        while i < self.projectiles.len() {
            if dead_proj_ids.contains(&self.projectiles[i].bound.physics_id) {
                let p = self.projectiles.swap_remove(i);
                p.bound
                    .despawn(&mut self.renderer, &mut self.physics, &mut self.entity_map);
            } else {
                i += 1;
            }
        }

        if player_hit
            && let Some(player) = &mut self.player
            && player.lives > 0
        {
            player.lives -= 1;
            if player.lives == 0 {
                self.game_over = true;
            }
        }
    }

    fn flush_pending_spawns(&mut self, gfx: &GfxState) -> Result<()> {
        let pending = std::mem::take(&mut self.pending_spawns);
        for spawn in pending {
            let mut ctx = SpawnCtx {
                renderer: &mut self.renderer,
                physics: &mut self.physics,
                atlas: &self.atlas,
                sprite_hulls: &self.sprite_hulls,
                entity_map: &mut self.entity_map,
                device: &gfx.device,
                queue: &gfx.queue,
            };
            match spawn {
                PendingSpawn::Projectile { pos, vel, owner } => {
                    let proj = spawn_projectile(&mut ctx, pos, vel, owner)?;
                    self.projectiles.push(proj);
                }
                PendingSpawn::Formation => {
                    let game_area = self.game_area;
                    self.enemies = spawn_formation(&mut ctx, game_area)?;
                    self.formation.direction = 1.0;
                    self.formation.step_timer = FORMATION_BASE_INTERVAL;
                    self.formation.step_interval = FORMATION_BASE_INTERVAL;
                }
            }
        }
        Ok(())
    }

    fn render_inner(&mut self, gfx: &GfxState) -> Result<()> {
        if self.pending_restart {
            *self = Self::new(gfx)?;
        }
        self.flush_pending_spawns(gfx)?;

        // Refresh enemy mesh UVs for current animation frame
        for i in 0..self.enemies.len() {
            let kind = self.enemies[i].kind;
            let frame = self.enemies[i].anim_frame;
            let actor_id = self.enemies[i].bound.actor_id;
            let sprite_name = enemy_sprite_name(kind, frame);
            let uv = self.atlas.get(sprite_name);
            let size = self.atlas.size(sprite_name);
            if let (Some(uv), Some((pw, ph))) = (uv, size) {
                let hw = pw as f32 * ENEMY_SCALE * 0.5;
                let hh = ph as f32 * ENEMY_SCALE * 0.5;
                if let Some(actor) = self.renderer.actor_mut(actor_id) {
                    actor.mesh.reset();
                    actor.mesh.append_quad(
                        &gfx.queue,
                        Vertex2d {
                            position: Vec2::new(-hw, -hh),
                            uv: Vec2::new(uv.min.x, uv.min.y),
                            color: Vec4::ONE,
                        },
                        Vertex2d {
                            position: Vec2::new(hw, -hh),
                            uv: Vec2::new(uv.max.x, uv.min.y),
                            color: Vec4::ONE,
                        },
                        Vertex2d {
                            position: Vec2::new(hw, hh),
                            uv: Vec2::new(uv.max.x, uv.max.y),
                            color: Vec4::ONE,
                        },
                        Vertex2d {
                            position: Vec2::new(-hw, hh),
                            uv: Vec2::new(uv.min.x, uv.max.y),
                            color: Vec4::ONE,
                        },
                    );
                }
            }
        }

        let output = gfx.surface.get_current_texture()?;
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = gfx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let current_lives = self.player.as_ref().map(|p| p.lives).unwrap_or(0);
        if current_lives != self.last_lives {
            self.last_lives = current_lives;
            self.lives_text.render_text(
                &gfx.device,
                &gfx.queue,
                &mut self.lives_mesh,
                &format!("Lives: {}", current_lives),
            );
            let new_texture = self.lives_text.texture.clone();
            if let Some(actor) = self.renderer.actor_mut(self.lives_actor_id) {
                std::mem::swap(&mut actor.mesh, &mut self.lives_mesh);
                actor.texture = new_texture;
            }
        }

        self.renderer.draw(
            gfx,
            &mut encoder,
            &view,
            Some(wgpu::Color {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            }),
        )?;

        if self.paused {
            self.pause_menu.draw(gfx, &mut encoder, &view, None)?;
        }
        if self.game_over {
            self.game_over_menu.draw(gfx, &mut encoder, &view, None)?;
        }

        gfx.queue.submit(std::iter::once(encoder.finish()));
        output.present();

        Ok(())
    }
}

impl State for GameState {
    fn update(&mut self, dt: std::time::Duration) -> Result<()> {
        self.update_inner(dt.as_secs_f32());
        Ok(())
    }

    fn render(&mut self, gfx: &GfxState) -> Result<()> {
        self.render_inner(gfx)
    }

    fn handle_event(&mut self, event: WindowEvent) -> Result<StateSwitch> {
        match event {
            WindowEvent::CloseRequested => return Ok(StateSwitch::ExitApplication),
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key, state: ElementState::Released, .. },
                ..
            } if self.game_over
                && !matches!(
                    logical_key,
                    Key::Named(
                        NamedKey::ArrowLeft
                            | NamedKey::ArrowRight
                            | NamedKey::ArrowUp
                            | NamedKey::ArrowDown
                    )
                ) =>
            {
                self.pending_restart = true;
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
                self.paused = !self.paused;
            }
            WindowEvent::KeyboardInput {
                event: KeyEvent {
                    logical_key, state, ..
                },
                ..
            } if !self.paused => {
                let pressed = state == ElementState::Pressed;
                match logical_key {
                    Key::Named(NamedKey::ArrowLeft) => self.input.left = pressed,
                    Key::Named(NamedKey::ArrowRight) => self.input.right = pressed,
                    Key::Named(NamedKey::Space) => self.input.fire = pressed,
                    _ => {}
                }
            }
            _ => {}
        }
        Ok(StateSwitch::NoChange)
    }
}
