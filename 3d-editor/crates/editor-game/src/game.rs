use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use editor_core::{Camera, CameraId, Extra, Projection};
use editor_geom::camera::look_at;
use editor_render::{PreparedScene, SceneEnv, ViewportOptions};
use editor_sim::{LoadWarning, Sim};
use glam::DVec3;

use crate::error::GameError;
use crate::gfx::Gfx;
use crate::input::Input;
use crate::session::Session;

const EYE_HEIGHT: f64 = 12.0;
const FOV: f64 = 75.0;
const MAX_STEP: f64 = 0.1;

pub struct Game {
    session: Session,
    sim: Sim,
    prepared: Option<PreparedScene>,
    pub camera: Camera,
    pub input: Input,
    pub paused: bool,
    last: Instant,
}

fn camera_at(position: DVec3, yaw: f64, pitch: f64) -> Camera {
    Camera {
        id: CameraId(0),
        name: "player".into(),
        position,
        yaw,
        pitch,
        projection: Projection::Perspective { fov: FOV },
        extra: Extra::new(),
    }
}

fn spawn_camera(sim: &Sim, session: &Session, size: (u32, u32)) -> Camera {
    if let Some(pose) = sim.spawn() {
        let forward = pose.rotation * DVec3::X;
        let (yaw, pitch) = look_at(DVec3::ZERO, forward).unwrap_or((0.0, 0.0));
        return camera_at(pose.translation + DVec3::Z * EYE_HEIGHT, yaw, pitch);
    }
    match editor_cmd::resolve_camera(&session.replica.doc, "viewport.persp", size.0, size.1) {
        Ok(camera) => Camera {
            projection: Projection::Perspective { fov: FOV },
            ..camera
        },
        Err(_) => camera_at(DVec3::new(0.0, -200.0, 100.0), 90.0, -10.0),
    }
}

impl Game {
    pub fn new(session: Session, size: (u32, u32)) -> (Self, Vec<LoadWarning>) {
        let (sim, warnings) = Sim::load(&session.replica.doc);
        let camera = spawn_camera(&sim, &session, size);
        let game = Self {
            sim,
            camera,
            session,
            prepared: None,
            input: Input::default(),
            paused: false,
            last: Instant::now(),
        };
        (game, warnings)
    }

    pub fn title(&self) -> String {
        let name = &self.session.replica.doc.name;
        let state = if self.paused { " [paused]" } else { "" };
        format!("{name} - t = {:.1}s{state}", self.sim.time())
    }

    pub fn closed(&self) -> bool {
        self.session.replica.closed
    }

    pub fn restart(&mut self) {
        self.sim.set_time(0.0);
    }

    /// Advances the simulation and the camera, and rebuilds what the server's edits invalidated.
    pub fn update(&mut self, gfx: &Gfx) -> Result<Vec<LoadWarning>, GameError> {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64().min(MAX_STEP);
        self.last = now;

        let applied = self.session.pump()?;
        let mut warnings = Vec::new();
        if applied.document || applied.defs || self.prepared.is_none() {
            let elapsed = self.sim.time();
            let (mut sim, w) = Sim::load(&self.session.replica.doc);
            sim.set_time(elapsed);
            self.sim = sim;
            warnings = w;
            self.prepared = None;
        }
        if !self.paused {
            self.sim.step(dt);
        }
        self.input.apply(&mut self.camera, dt);
        if self.prepared.is_none() {
            let replica = &self.session.replica;
            let scene_dir = replica
                .file
                .path
                .as_deref()
                .and_then(|p| Path::new(p).parent());
            let looks = HashMap::new();
            let faces = Default::default();
            let env = SceneEnv {
                scene_dir,
                textures: gfx.renderer.textures(),
                faces: &faces,
                looks: &looks,
                links: &[],
            };
            self.prepared = Some(PreparedScene::game(&replica.doc, self.sim.excluded(), &env));
        }
        Ok(warnings)
    }

    pub fn draw(&self, gfx: &mut Gfx) {
        let Some(prepared) = &self.prepared else {
            return;
        };
        let poses = self.sim.poses();
        let camera = &self.camera;
        gfx.frame(|renderer, target, (w, h)| {
            let view = prepared.view(camera, w, h);
            renderer.draw_viewport_posed(
                prepared,
                &view,
                ViewportOptions {
                    grid: None,
                    axes: false,
                },
                (w, h),
                target,
                &poses,
            );
        });
    }
}
