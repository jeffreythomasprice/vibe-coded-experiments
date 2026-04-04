use crate::camera::Rect;
use crate::graphics::immediate::{Color, ColorVertex2D};
use crate::graphics::vector::{self, FillOptions, Mesh, PathBuilder};

use super::{GridArt, ParamDescription};

pub struct HueCircle;

impl GridArt for HueCircle {
    fn param_count(&self) -> usize {
        1
    }

    fn describe(&self, params: &[f32]) -> Vec<ParamDescription> {
        let hue = params[0];
        let color = Color::from_hsv(hue, 1.0, 1.0);
        vec![ParamDescription {
            name: "Color".into(),
            value: format!("{}", color),
        }]
    }

    fn cell_mesh(&self, cell_rect: Rect, params: &[f32]) -> Mesh<ColorVertex2D> {
        let hue = params[0];
        let color = Color::from_hsv(hue, 1.0, 1.0);
        let center = cell_rect.center();
        let radius = cell_rect.width().min(cell_rect.height()) / 2.0;

        let mut builder = PathBuilder::new();
        builder.circle(center, radius, 32);
        let path = builder.build();
        vector::fill(&path, color, &FillOptions::default())
    }
}
