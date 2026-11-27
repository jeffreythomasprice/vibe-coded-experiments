mod canvas;
mod creature;
mod font;
mod hound;
mod human;
mod palette;
mod pixbuf;
mod rng;
mod stamp;

use std::cell::RefCell;
use std::rc::Rc;

use gloo::events::EventListener;
use gloo::render::{AnimationFrame, request_animation_frame};
use gloo::utils::{document, window};
use wasm_bindgen::{Clamped, JsCast, JsValue};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, ImageData, KeyboardEvent};

use creature::{Anim, Creature, Facing, Kind, Reach};
use stamp::Stamps;

/// Walking speed in canvas pixels per second.
const WALK_SPEED: f64 = 24.0;

struct Game {
    /// The visible, window-sized canvas.
    screen: (HtmlCanvasElement, CanvasRenderingContext2d),
    /// The low-resolution canvas that gets scaled up onto the screen.
    scene: (HtmlCanvasElement, CanvasRenderingContext2d),
    /// Canvas pixels per art pixel.
    scale: u32,
    /// Scene size in art pixels.
    width: i32,
    height: i32,
    stamps: Stamps,
    seed: u64,
    /// One of each kind, so switching back keeps its toggles.
    creatures: [Creature; Kind::ALL.len()],
    /// Index of the one on screen.
    shown: usize,
    /// Position of the creature's feet.
    x: f64,
    y: f64,
    /// Arrow keys currently down, most recent last.
    held: Vec<Facing>,
    facing: Facing,
    anim: Anim,
    anim_start: f64,
    last_frame: Option<f64>,
}

impl Game {
    /// Follows window size changes.
    fn resize(&mut self) {
        let (canvas, ctx) = &self.screen;
        canvas::fit_to_window(canvas, ctx);
        let (scale, width, height) = canvas::art_scale(canvas.width(), canvas.height());
        self.scale = scale;
        if (self.width, self.height) != (width, height) {
            (self.width, self.height) = (width, height);
            self.scene.0.set_width(width as u32);
            self.scene.0.set_height(height as u32);
        }
    }

    fn key_down(&mut self, key: &str) {
        if let Some(direction) = arrow(key) {
            if !self.held.contains(&direction) {
                self.held.push(direction);
            }
            return;
        }
        match key {
            "r" | "R" => {
                self.seed += 1;
                self.creatures = spawn(self.seed);
            }
            "t" | "T" => self.shown = (self.shown + 1) % self.creatures.len(),
            "h" | "H" => self.creatures[self.shown].toggle_extra(),
            _ => {}
        }
    }

    fn key_up(&mut self, key: &str) {
        if let Some(direction) = arrow(key) {
            self.held.retain(|&d| d != direction);
        }
    }

    /// Advances to `now`, in seconds.
    fn update(&mut self, now: f64) {
        // Capped so a backgrounded tab does not teleport the character.
        let dt = self.last_frame.map_or(0.0, |last| (now - last).min(0.1));
        self.last_frame = Some(now);

        let anim = if let Some(&direction) = self.held.last() {
            self.facing = direction;
            let step = WALK_SPEED * dt;
            match direction {
                Facing::North => self.y -= step,
                Facing::South => self.y += step,
                Facing::West => self.x -= step,
                Facing::East => self.x += step,
            }
            Anim::Walk
        } else {
            Anim::Idle
        };
        // Keep the whole sprite on screen. Not `clamp`, which panics if a tiny
        // window puts the maximum below the minimum.
        let Reach { side, above, below } = self.creatures[self.shown].reach();
        self.x = self
            .x
            .min(f64::from(self.width - side))
            .max(f64::from(side));
        self.y = self
            .y
            .min(f64::from(self.height - below))
            .max(f64::from(above));
        if anim != self.anim {
            self.anim = anim;
            self.anim_start = now;
        }
    }

    fn draw(&self, now: f64) -> Result<(), JsValue> {
        let (width, height) = (self.width, self.height);
        let mut frame = background(width, height);

        let (x, y) = (self.x.round() as i32, self.y.round() as i32);
        let creature = &self.creatures[self.shown];
        shadow(&mut frame, width, x, y, creature.shadow(self.facing));
        let (origin_x, ground_y) = creature.origin();
        creature
            .render(&self.stamps, self.facing, self.anim, now - self.anim_start)
            .paint(
                &mut frame,
                width,
                x - origin_x,
                y - ground_y,
                creature.palette(),
            );

        let image = ImageData::new_with_u8_clamped_array_and_sh(
            Clamped(&frame),
            width as u32,
            height as u32,
        )?;
        let (scene, scene_ctx) = &self.scene;
        scene_ctx.put_image_data(&image, 0.0, 0.0)?;

        let label = format!("SEED {}", self.seed);
        scene_ctx.set_fill_style_str("#fff1e8");
        let left = (width - font::text_width(&label) as i32) / 2;
        font::draw_text(scene_ctx, &label, f64::from(left), 3.0, 1.0);

        let scale = f64::from(self.scale);
        self.screen
            .1
            .draw_image_with_html_canvas_element_and_dw_and_dh(
                scene,
                0.0,
                0.0,
                f64::from(width) * scale,
                f64::from(height) * scale,
            )
    }
}

fn arrow(key: &str) -> Option<Facing> {
    match key {
        "ArrowUp" => Some(Facing::North),
        "ArrowDown" => Some(Facing::South),
        "ArrowLeft" => Some(Facing::West),
        "ArrowRight" => Some(Facing::East),
        _ => None,
    }
}

/// An RGBA grass field with scattered tufts.
fn background(width: i32, height: i32) -> Vec<u8> {
    let mut frame = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let tuft = (x * 37 + y * 91 + x * y * 7) % 53 == 0;
            let color: [u8; 4] = if tuft {
                [0x6c, 0xa4, 0x58, 255]
            } else {
                [0x58, 0x90, 0x50, 255]
            };
            frame.extend_from_slice(&color);
        }
    }
    frame
}

fn spawn(seed: u64) -> [Creature; Kind::ALL.len()] {
    Kind::ALL.map(|kind| Creature::random(kind, seed))
}

/// Darkens a flat ellipse under the creature's feet, given as (row, half width).
fn shadow(frame: &mut [u8], width: i32, center_x: i32, center_y: i32, rows: &[(i32, i32)]) {
    for &(row, half) in rows {
        for x in center_x - half..center_x + half {
            let i = ((center_y + row) * width + x) * 4;
            let Some(pixel) = usize::try_from(i)
                .ok()
                .and_then(|i| frame.get_mut(i..i + 3))
            else {
                continue;
            };
            for channel in pixel {
                *channel = (u16::from(*channel) * 3 / 4) as u8;
            }
        }
    }
}

/// Runs one frame and schedules the next. The handle is parked in `pending`
/// because dropping it would cancel the request.
fn run(game: Rc<RefCell<Game>>, pending: Rc<RefCell<Option<AnimationFrame>>>) {
    let next = pending.clone();
    let handle = request_animation_frame(move |millis| {
        let now = millis / 1000.0;
        {
            let mut game = game.borrow_mut();
            game.resize();
            game.update(now);
            if let Err(error) = game.draw(now) {
                log::error!("draw failed: {error:?}");
                return;
            }
        }
        run(game, next);
    });
    *pending.borrow_mut() = Some(handle);
}

fn listen_for_keys(game: &Rc<RefCell<Game>>) {
    let on_key = |event_type: &'static str, handle: fn(&mut Game, &str)| {
        let game = game.clone();
        EventListener::new(&document(), event_type, move |event| {
            if let Some(event) = event.dyn_ref::<KeyboardEvent>() {
                handle(&mut game.borrow_mut(), &event.key());
            }
        })
        .forget();
    };
    on_key("keydown", Game::key_down);
    on_key("keyup", Game::key_up);

    // Key-up events are lost when focus leaves the page.
    let game = game.clone();
    EventListener::new(&window(), "blur", move |_| game.borrow_mut().held.clear()).forget();
}

fn main() {
    console_error_panic_hook::set_once();
    console_log::init_with_level(log::Level::Debug).expect("logger already initialized");

    log::info!("starting up");

    let screen = canvas::canvas_2d("canvas").expect("failed to set up canvas");
    canvas::fit_to_window(&screen.0, &screen.1);
    let (_, width, height) = canvas::art_scale(screen.0.width(), screen.0.height());
    let seed = 1;
    let mut game = Game {
        screen,
        scene: canvas::offscreen_2d().expect("failed to create the scene canvas"),
        scale: 1,
        // Zero so the first resize sizes the scene canvas.
        width: 0,
        height: 0,
        stamps: Stamps::load().expect("bad stamp file"),
        seed,
        creatures: spawn(seed),
        shown: 0,
        x: f64::from(width / 2),
        y: f64::from(height * 2 / 3),
        held: Vec::new(),
        facing: Facing::South,
        anim: Anim::Idle,
        anim_start: 0.0,
        last_frame: None,
    };
    game.resize();

    let game = Rc::new(RefCell::new(game));
    listen_for_keys(&game);
    run(game, Rc::default());
}
