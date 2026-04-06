mod generator_enum;
mod hue_circle;
mod param;
mod perlin_noise;

pub use generator_enum::ArtGeneratorGrid;
pub use hue_circle::HueCircle;
pub use param::*;
pub use perlin_noise::PerlinNoise;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

use glam::{Mat4, Vec2};

use crate::camera::Rect;
use crate::graphics::immediate::{Color, ColorVertex2D, Frame};
use crate::graphics::vector::{self, LineCap, Mesh, PathBuilder, StrokeOptions};

const MAX_CONCURRENT_INITS: usize = 5;

pub struct ParamDescription {
    pub name: String,
    pub value: String,
}

pub trait ParameterizedGraphics {
    type Params;
    type Instance;

    fn description(&self) -> &str;
    fn param_defs(&self) -> Vec<ParamDef>;
    fn build_params(&self, values: &[ParamValue]) -> Self::Params;
    fn init(&self, params: &Self::Params, device: &wgpu::Device, queue: &wgpu::Queue) -> Self::Instance;
    fn render<'a>(&self, instance: &'a Self::Instance, cell_rect: Rect, frame: &mut Frame<'a>);
}

pub enum GridParamMapping {
    One(usize),
    Two(usize, usize),
}

pub struct ParameterizedGraphicsGrid<A: ParameterizedGraphics> {
    art: Arc<A>,
    cols: usize,
    rows: usize,
    bounds: Rect,
    cell_width: f32,
    cell_height: f32,
    gap: f32,
    hovered: Option<(usize, usize)>,
    grid_params: GridParamMapping,
    fixed_values: Vec<ParamValue>,
    param_ranges: Vec<ParamRange>,
    instances: Vec<Option<A::Instance>>,
    receiver: Option<Receiver<(usize, A::Instance)>>,
    pending_count: usize,
    elapsed_time: f32,
    cancelled: Arc<AtomicBool>,
}

impl<A> ParameterizedGraphicsGrid<A>
where
    A: ParameterizedGraphics + Send + Sync + 'static,
    A::Params: Send + 'static,
    A::Instance: Send + 'static,
{
    pub fn new(
        art: A,
        cols: usize,
        rows: usize,
        bounds: Rect,
        gap: f32,
        grid_params: GridParamMapping,
        fixed_values: Vec<ParamValue>,
        param_ranges: Option<Vec<ParamRange>>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {
        let defs = art.param_defs();
        let param_count = defs.len();
        assert_eq!(
            fixed_values.len(),
            param_count,
            "fixed_values length must match param_defs count"
        );
        match &grid_params {
            GridParamMapping::One(idx) => {
                assert!(*idx < param_count, "grid param index out of range");
            }
            GridParamMapping::Two(x, y) => {
                assert!(*x < param_count && *y < param_count, "grid param index out of range");
                assert!(x != y, "grid param indices must be different");
            }
        }

        let param_ranges = param_ranges
            .unwrap_or_else(|| defs.iter().map(|d| d.range).collect());

        let cell_width = bounds.width() / cols as f32;
        let cell_height = bounds.height() / rows as f32;
        let art = Arc::new(art);
        let total = cols * rows;

        let cancelled = Arc::new(AtomicBool::new(false));

        let mut grid = Self {
            art: art.clone(),
            cols,
            rows,
            bounds,
            cell_width,
            cell_height,
            gap,
            hovered: None,
            grid_params,
            fixed_values,
            param_ranges,
            instances: (0..total).map(|_| None).collect(),
            receiver: None,
            pending_count: total,
            elapsed_time: 0.0,
            cancelled: cancelled.clone(),
        };

        let (tx, rx) = mpsc::channel();
        let semaphore = Arc::new((Mutex::new(0usize), Condvar::new()));
        let device = device.clone();
        let queue = queue.clone();

        for row in 0..rows {
            for col in 0..cols {
                let idx = row * cols + col;
                let values = grid.params_for(col, row);
                let params = grid.art.build_params(&values);

                let art = art.clone();
                let tx = tx.clone();
                let sem = semaphore.clone();
                let device = device.clone();
                let queue = queue.clone();
                let cancelled = cancelled.clone();

                thread::spawn(move || {
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }

                    {
                        let (lock, cvar) = &*sem;
                        let mut count = lock.lock().unwrap();
                        while *count >= MAX_CONCURRENT_INITS {
                            if cancelled.load(Ordering::Relaxed) {
                                return;
                            }
                            count = cvar.wait(count).unwrap();
                        }
                        *count += 1;
                    }

                    if cancelled.load(Ordering::Relaxed) {
                        let (lock, cvar) = &*sem;
                        let mut count = lock.lock().unwrap();
                        *count -= 1;
                        cvar.notify_one();
                        return;
                    }

                    let instance = art.init(&params, &device, &queue);
                    let _ = tx.send((idx, instance));

                    {
                        let (lock, cvar) = &*sem;
                        let mut count = lock.lock().unwrap();
                        *count -= 1;
                        cvar.notify_one();
                    }
                });
            }
        }

        grid.receiver = Some(rx);
        grid
    }

    pub fn poll_inits(&mut self) {
        if let Some(rx) = &self.receiver {
            while let Ok((idx, instance)) = rx.try_recv() {
                self.instances[idx] = Some(instance);
                self.pending_count -= 1;
            }
            if self.pending_count == 0 {
                self.receiver = None;
            }
        }
    }

    pub fn update_time(&mut self, dt: f32) {
        self.elapsed_time += dt;
    }

    pub fn art(&self) -> &A {
        &self.art
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    pub fn cols(&self) -> usize {
        self.cols
    }

    pub fn rows(&self) -> usize {
        self.rows
    }

    pub fn param_ranges(&self) -> &[ParamRange] {
        &self.param_ranges
    }

    fn cell_outer_rect(&self, col: usize, row: usize) -> Rect {
        let min =
            self.bounds.min + Vec2::new(col as f32 * self.cell_width, row as f32 * self.cell_height);
        Rect::new(min, min + Vec2::new(self.cell_width, self.cell_height))
    }

    fn cell_inner_rect(&self, col: usize, row: usize) -> Rect {
        let outer = self.cell_outer_rect(col, row);
        Rect::new(
            outer.min + Vec2::splat(self.gap),
            outer.max - Vec2::splat(self.gap),
        )
    }

    fn params_for(&self, col: usize, row: usize) -> Vec<ParamValue> {
        let mut values = self.fixed_values.clone();

        match self.grid_params {
            GridParamMapping::One(idx) => {
                let total = self.cols * self.rows;
                let index = row * self.cols + col;
                let t = if total <= 1 {
                    0.0
                } else {
                    index as f32 / (total - 1) as f32
                };
                values[idx] = self.param_ranges[idx].lerp(t);
            }
            GridParamMapping::Two(x_idx, y_idx) => {
                let tx = if self.cols <= 1 {
                    0.0
                } else {
                    col as f32 / (self.cols - 1) as f32
                };
                let ty = if self.rows <= 1 {
                    0.0
                } else {
                    row as f32 / (self.rows - 1) as f32
                };
                values[x_idx] = self.param_ranges[x_idx].lerp(tx);
                values[y_idx] = self.param_ranges[y_idx].lerp(ty);
            }
        }

        values
    }

    fn hit_test(&self, world_pos: Vec2) -> Option<(usize, usize)> {
        if !self.bounds.contains(world_pos) {
            return None;
        }
        let rel = world_pos - self.bounds.min;
        let col = (rel.x / self.cell_width) as usize;
        let row = (rel.y / self.cell_height) as usize;
        if col < self.cols && row < self.rows {
            Some((col, row))
        } else {
            None
        }
    }

    pub fn update_hover(&mut self, world_pos: Vec2) {
        self.hovered = self.hit_test(world_pos);
    }

    pub fn render_cells<'a>(&'a self, frame: &mut Frame<'a>) {
        for row in 0..self.rows {
            for col in 0..self.cols {
                let idx = row * self.cols + col;
                let inner = self.cell_inner_rect(col, row);
                match &self.instances[idx] {
                    Some(instance) => self.art.render(instance, inner, frame),
                    None => Self::render_spinner(inner, self.elapsed_time, frame),
                }
            }
        }
    }

    fn render_spinner(cell_rect: Rect, elapsed: f32, frame: &mut Frame<'_>) {
        let center = cell_rect.center();
        let radius = cell_rect.width().min(cell_rect.height()) * 0.3;
        let rotation_speed = 2.0;
        let base_angle = elapsed * rotation_speed;
        let arc_sweep = std::f32::consts::TAU * 0.9;
        let segments = 32u32;
        let spinner_color = Color::rgba(0.8, 0.8, 0.8, 0.6);
        let stroke_width = radius * 0.15;

        let mut builder = PathBuilder::new();
        for i in 0..=segments {
            let t = i as f32 / segments as f32;
            let angle = base_angle + t * arc_sweep;
            let p = center + Vec2::new(angle.cos(), angle.sin()) * radius;
            if i == 0 {
                builder.move_to(p);
            } else {
                builder.line_to(p);
            }
        }
        builder.end();
        let arc_path = builder.build();
        let options = StrokeOptions::default()
            .with_line_width(stroke_width)
            .with_line_cap(LineCap::Butt);
        let arc_mesh = vector::stroke(&arc_path, spinner_color, &options);

        let mut mat = frame.color_material(Color::WHITE, Mat4::IDENTITY);
        mat.indexed_triangle_list(&arc_mesh.vertices, &arc_mesh.indices);
    }

    pub fn draw_hover_outline(&self) -> Option<Mesh<ColorVertex2D>> {
        let (col, row) = self.hovered?;
        let outer = self.cell_outer_rect(col, row);
        let mut builder = PathBuilder::new();
        builder.rect(outer.min, outer.size());
        let path = builder.build();
        let options = StrokeOptions::default().with_line_width(1.5);
        Some(vector::stroke(&path, Color::WHITE, &options))
    }

    pub fn hover_description(&self) -> Option<Vec<ParamDescription>> {
        let (col, row) = self.hovered?;
        let values = self.params_for(col, row);
        let defs = self.art.param_defs();

        Some(
            defs.iter()
                .zip(values.iter())
                .map(|(def, val)| ParamDescription {
                    name: def.description.clone(),
                    value: def.format_value(val),
                })
                .collect(),
        )
    }
}

impl<A: ParameterizedGraphics> Drop for ParameterizedGraphicsGrid<A> {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
