use glam::Vec2;

use crate::camera::Rect;
use crate::graphics::immediate::{ColorVertex2D, Frame};
use crate::graphics::vector::Mesh;

use super::{
    GridParamMapping, HueCircle, ParamDef, ParamDescription, ParamRange, ParamValue,
    ParameterizedGraphics, ParameterizedGraphicsGrid, PerlinNoise,
};

pub enum ArtGeneratorGrid {
    HueCircle(ParameterizedGraphicsGrid<HueCircle>),
    PerlinNoise(ParameterizedGraphicsGrid<PerlinNoise>),
}

fn default_fixed_values(defs: &[ParamDef]) -> Vec<ParamValue> {
    defs.iter().map(|d| d.range.lerp(0.5)).collect()
}

impl ArtGeneratorGrid {
    pub fn new(
        generator_index: usize,
        cols: usize,
        rows: usize,
        bounds: Rect,
        gap: f32,
        param_ranges: Option<Vec<ParamRange>>,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> Self {
        match generator_index {
            0 => {
                let defs = HueCircle.param_defs();
                let fixed = default_fixed_values(&defs);
                Self::HueCircle(ParameterizedGraphicsGrid::new(
                    HueCircle, cols, rows, bounds, gap,
                    GridParamMapping::One(0), fixed, param_ranges, device, queue,
                ))
            }
            1 => {
                let defs = PerlinNoise.param_defs();
                let fixed = default_fixed_values(&defs);
                Self::PerlinNoise(ParameterizedGraphicsGrid::new(
                    PerlinNoise, cols, rows, bounds, gap,
                    GridParamMapping::One(0), fixed, param_ranges, device, queue,
                ))
            }
            _ => panic!("Unknown generator index: {generator_index}"),
        }
    }

    pub fn param_defs(&self) -> Vec<ParamDef> {
        match self {
            Self::HueCircle(g) => g.art().param_defs(),
            Self::PerlinNoise(g) => g.art().param_defs(),
        }
    }

    pub fn rows(&self) -> usize {
        match self {
            Self::HueCircle(g) => g.rows(),
            Self::PerlinNoise(g) => g.rows(),
        }
    }

    pub fn cols(&self) -> usize {
        match self {
            Self::HueCircle(g) => g.cols(),
            Self::PerlinNoise(g) => g.cols(),
        }
    }

    pub fn bounds(&self) -> Rect {
        match self {
            Self::HueCircle(g) => g.bounds(),
            Self::PerlinNoise(g) => g.bounds(),
        }
    }

    pub fn param_ranges(&self) -> Vec<ParamRange> {
        match self {
            Self::HueCircle(g) => g.param_ranges().to_vec(),
            Self::PerlinNoise(g) => g.param_ranges().to_vec(),
        }
    }

    pub fn update_hover(&mut self, world_pos: Vec2) {
        match self {
            Self::HueCircle(g) => g.update_hover(world_pos),
            Self::PerlinNoise(g) => g.update_hover(world_pos),
        }
    }

    pub fn update_time(&mut self, dt: f32) {
        match self {
            Self::HueCircle(g) => g.update_time(dt),
            Self::PerlinNoise(g) => g.update_time(dt),
        }
    }

    pub fn poll_inits(&mut self) {
        match self {
            Self::HueCircle(g) => g.poll_inits(),
            Self::PerlinNoise(g) => g.poll_inits(),
        }
    }

    pub fn render_cells<'a>(&'a self, frame: &mut Frame<'a>) {
        match self {
            Self::HueCircle(g) => g.render_cells(frame),
            Self::PerlinNoise(g) => g.render_cells(frame),
        }
    }

    pub fn draw_hover_outline(&self) -> Option<Mesh<ColorVertex2D>> {
        match self {
            Self::HueCircle(g) => g.draw_hover_outline(),
            Self::PerlinNoise(g) => g.draw_hover_outline(),
        }
    }

    pub fn hover_description(&self) -> Option<Vec<ParamDescription>> {
        match self {
            Self::HueCircle(g) => g.hover_description(),
            Self::PerlinNoise(g) => g.hover_description(),
        }
    }
}
