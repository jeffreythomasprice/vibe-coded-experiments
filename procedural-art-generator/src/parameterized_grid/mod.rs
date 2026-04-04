mod hue_circle;

pub use hue_circle::HueCircle;

use glam::Vec2;

use crate::camera::Rect;
use crate::graphics::immediate::{Color, ColorVertex2D};
use crate::graphics::vector::{self, Mesh, PathBuilder, StrokeOptions};

pub struct ParamDescription {
    pub name: String,
    pub value: String,
}

pub trait GridArt {
    fn param_count(&self) -> usize;
    fn describe(&self, params: &[f32]) -> Vec<ParamDescription>;
    fn cell_mesh(&self, cell_rect: Rect, params: &[f32]) -> Mesh<ColorVertex2D>;
}

pub struct ArtGrid<A: GridArt> {
    art: A,
    cols: usize,
    rows: usize,
    bounds: Rect,
    cell_width: f32,
    cell_height: f32,
    gap: f32,
    hovered: Option<(usize, usize)>,
}

impl<A: GridArt> ArtGrid<A> {
    pub fn new(art: A, cols: usize, rows: usize, bounds: Rect, gap: f32) -> Self {
        let cell_width = bounds.width() / cols as f32;
        let cell_height = bounds.height() / rows as f32;
        Self {
            art,
            cols,
            rows,
            bounds,
            cell_width,
            cell_height,
            gap,
            hovered: None,
        }
    }

    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    fn cell_outer_rect(&self, col: usize, row: usize) -> Rect {
        let min = self.bounds.min + Vec2::new(col as f32 * self.cell_width, row as f32 * self.cell_height);
        Rect::new(min, min + Vec2::new(self.cell_width, self.cell_height))
    }

    fn cell_inner_rect(&self, col: usize, row: usize) -> Rect {
        let outer = self.cell_outer_rect(col, row);
        Rect::new(
            outer.min + Vec2::splat(self.gap),
            outer.max - Vec2::splat(self.gap),
        )
    }

    fn params_for(&self, col: usize, row: usize) -> Vec<f32> {
        if self.art.param_count() == 1 {
            let total = self.cols * self.rows;
            let index = row * self.cols + col;
            let t = if total <= 1 { 0.0 } else { index as f32 / (total - 1) as f32 };
            vec![t]
        } else {
            let tx = if self.cols <= 1 { 0.0 } else { col as f32 / (self.cols - 1) as f32 };
            let ty = if self.rows <= 1 { 0.0 } else { row as f32 / (self.rows - 1) as f32 };
            vec![tx, ty]
        }
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

    pub fn draw_cells(&self) -> Mesh<ColorVertex2D> {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        for row in 0..self.rows {
            for col in 0..self.cols {
                let params = self.params_for(col, row);
                let inner = self.cell_inner_rect(col, row);
                let mesh = self.art.cell_mesh(inner, &params);

                let base = vertices.len() as u16;
                vertices.extend_from_slice(&mesh.vertices);
                indices.extend(mesh.indices.iter().map(|i| i + base));
            }
        }

        Mesh { vertices, indices }
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
        let params = self.params_for(col, row);
        Some(self.art.describe(&params))
    }
}
