
use glam::{Mat4, Vec2, Vec3};

use crate::camera::Rect;
use crate::graphics::immediate::{Color, ColorVertex2D, Frame};
use crate::graphics::vector::{self, FillOptions, Mesh, PathBuilder};

use super::{ParamDef, ParamRange, ParamValue, ParameterizedGraphics};

pub struct HueCircle;

pub struct HueCircleInstance {
    mesh: Mesh<ColorVertex2D>,
}

impl ParameterizedGraphics for HueCircle {
    type Params = Color;
    type Instance = HueCircleInstance;

    fn description(&self) -> &str {
        "A test demonstrating a single parameterized value"
    }

    fn param_defs(&self) -> Vec<ParamDef> {
        vec![ParamDef {
            description: "Hue".into(),
            range: ParamRange::F32 { min: 0.0, max: 1.0 },
            formatter: Some(Box::new(|v| {
                let hue = v.as_f32();
                format!("{}", Color::from_hsv(hue, 1.0, 1.0))
            })),
        }]
    }

    fn build_params(&self, values: &[ParamValue]) -> Color {
        Color::from_hsv(values[0].as_f32(), 1.0, 1.0)
    }

    fn init(
        &self,
        color: &Color,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
    ) -> HueCircleInstance {
        let mut builder = PathBuilder::new();
        builder.circle(Vec2::ZERO, 1.0, 32);
        let path = builder.build();
        let mesh = vector::fill(&path, *color, &FillOptions::default());
        HueCircleInstance { mesh }
    }

    fn render<'a>(&self, instance: &'a HueCircleInstance, cell_rect: Rect, frame: &mut Frame<'a>) {
        let center = cell_rect.center();
        let radius = cell_rect.width().min(cell_rect.height()) / 2.0;
        let transform = Mat4::from_translation(Vec3::new(center.x, center.y, 0.0))
            * Mat4::from_scale(Vec3::new(radius, radius, 1.0));
        let mut mat = frame.color_material(Color::WHITE, transform);
        mat.indexed_triangle_list(&instance.mesh.vertices, &instance.mesh.indices);
    }
}
