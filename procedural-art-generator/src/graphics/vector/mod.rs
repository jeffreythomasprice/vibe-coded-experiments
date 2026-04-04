use glam::Vec2;
use lyon::lyon_tessellation::{
    BuffersBuilder, FillTessellator, FillVertex, FillVertexConstructor, StrokeTessellator,
    StrokeVertex, StrokeVertexConstructor, VertexBuffers,
};
use lyon::path::Path;

use super::immediate::{Color, ColorVertex2D};

pub use lyon::lyon_tessellation::{FillOptions, FillRule, LineCap, LineJoin, StrokeOptions};
pub use lyon::path;

// ---------------------------------------------------------------------------
// Mesh output
// ---------------------------------------------------------------------------

pub struct Mesh<V> {
    pub vertices: Vec<V>,
    pub indices: Vec<u16>,
}

// ---------------------------------------------------------------------------
// PathBuilder — ergonomic wrapper over lyon's path builder using glam::Vec2
// ---------------------------------------------------------------------------

pub struct PathBuilder {
    inner: lyon::path::path::Builder,
}

impl PathBuilder {
    pub fn new() -> Self {
        Self {
            inner: Path::builder(),
        }
    }

    pub fn move_to(&mut self, pos: Vec2) -> &mut Self {
        self.inner.begin(lyon::math::point(pos.x, pos.y));
        self
    }

    pub fn line_to(&mut self, pos: Vec2) -> &mut Self {
        self.inner.line_to(lyon::math::point(pos.x, pos.y));
        self
    }

    pub fn quadratic_bezier_to(&mut self, ctrl: Vec2, to: Vec2) -> &mut Self {
        self.inner.quadratic_bezier_to(
            lyon::math::point(ctrl.x, ctrl.y),
            lyon::math::point(to.x, to.y),
        );
        self
    }

    pub fn cubic_bezier_to(&mut self, ctrl1: Vec2, ctrl2: Vec2, to: Vec2) -> &mut Self {
        self.inner.cubic_bezier_to(
            lyon::math::point(ctrl1.x, ctrl1.y),
            lyon::math::point(ctrl2.x, ctrl2.y),
            lyon::math::point(to.x, to.y),
        );
        self
    }

    pub fn close(&mut self) -> &mut Self {
        self.inner.close();
        self
    }

    pub fn circle(&mut self, center: Vec2, radius: f32, segments: u32) -> &mut Self {
        for i in 0..segments {
            let angle = (i as f32 / segments as f32) * std::f32::consts::TAU;
            let p = center + Vec2::new(angle.cos(), angle.sin()) * radius;
            if i == 0 {
                self.move_to(p);
            } else {
                self.line_to(p);
            }
        }
        self.close()
    }

    pub fn rect(&mut self, top_left: Vec2, size: Vec2) -> &mut Self {
        self.move_to(top_left);
        self.line_to(top_left + Vec2::new(size.x, 0.0));
        self.line_to(top_left + size);
        self.line_to(top_left + Vec2::new(0.0, size.y));
        self.close()
    }

    pub fn build(self) -> Path {
        self.inner.build()
    }
}

// ---------------------------------------------------------------------------
// Vertex constructors
// ---------------------------------------------------------------------------

struct ColorFillCtor {
    color: Color,
}

impl FillVertexConstructor<ColorVertex2D> for ColorFillCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> ColorVertex2D {
        ColorVertex2D {
            position: Vec2::new(vertex.position().x, vertex.position().y),
            color: self.color,
        }
    }
}

struct ColorStrokeCtor {
    color: Color,
}

impl StrokeVertexConstructor<ColorVertex2D> for ColorStrokeCtor {
    fn new_vertex(&mut self, vertex: StrokeVertex) -> ColorVertex2D {
        ColorVertex2D {
            position: Vec2::new(vertex.position().x, vertex.position().y),
            color: self.color,
        }
    }
}

struct GradientFillCtor {
    start_pos: Vec2,
    axis: Vec2,
    axis_len_sq: f32,
    start_color: Color,
    end_color: Color,
}

impl GradientFillCtor {
    fn new(start_pos: Vec2, end_pos: Vec2, start_color: Color, end_color: Color) -> Self {
        let axis = end_pos - start_pos;
        Self {
            start_pos,
            axis,
            axis_len_sq: axis.length_squared(),
            start_color,
            end_color,
        }
    }

    fn color_at(&self, pos: Vec2) -> Color {
        let t = if self.axis_len_sq < 1e-10 {
            0.0
        } else {
            ((pos - self.start_pos).dot(self.axis) / self.axis_len_sq).clamp(0.0, 1.0)
        };
        Color::rgba(
            self.start_color.r + (self.end_color.r - self.start_color.r) * t,
            self.start_color.g + (self.end_color.g - self.start_color.g) * t,
            self.start_color.b + (self.end_color.b - self.start_color.b) * t,
            self.start_color.a + (self.end_color.a - self.start_color.a) * t,
        )
    }
}

impl FillVertexConstructor<ColorVertex2D> for GradientFillCtor {
    fn new_vertex(&mut self, vertex: FillVertex) -> ColorVertex2D {
        let pos = Vec2::new(vertex.position().x, vertex.position().y);
        ColorVertex2D {
            position: pos,
            color: self.color_at(pos),
        }
    }
}

// ---------------------------------------------------------------------------
// Tessellation functions
// ---------------------------------------------------------------------------

pub fn fill(path: &Path, color: Color, options: &FillOptions) -> Mesh<ColorVertex2D> {
    let mut buffers: VertexBuffers<ColorVertex2D, u16> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();
    tessellator
        .tessellate_path(
            path,
            options,
            &mut BuffersBuilder::new(&mut buffers, ColorFillCtor { color }),
        )
        .expect("fill tessellation failed");
    Mesh {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}

pub fn stroke(path: &Path, color: Color, options: &StrokeOptions) -> Mesh<ColorVertex2D> {
    let mut buffers: VertexBuffers<ColorVertex2D, u16> = VertexBuffers::new();
    let mut tessellator = StrokeTessellator::new();
    tessellator
        .tessellate_path(
            path,
            options,
            &mut BuffersBuilder::new(&mut buffers, ColorStrokeCtor { color }),
        )
        .expect("stroke tessellation failed");
    Mesh {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}

pub fn fill_gradient(
    path: &Path,
    start_pos: Vec2,
    start_color: Color,
    end_pos: Vec2,
    end_color: Color,
    options: &FillOptions,
) -> Mesh<ColorVertex2D> {
    let mut buffers: VertexBuffers<ColorVertex2D, u16> = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();
    tessellator
        .tessellate_path(
            path,
            options,
            &mut BuffersBuilder::new(
                &mut buffers,
                GradientFillCtor::new(start_pos, end_pos, start_color, end_color),
            ),
        )
        .expect("fill_gradient tessellation failed");
    Mesh {
        vertices: buffers.vertices,
        indices: buffers.indices,
    }
}
