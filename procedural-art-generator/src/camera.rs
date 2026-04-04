use glam::Vec2;

#[derive(Clone, Copy, Debug)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn new(min: Vec2, max: Vec2) -> Self {
        Self { min, max }
    }

    pub fn from_center_size(center: Vec2, size: Vec2) -> Self {
        let half = size / 2.0;
        Self {
            min: center - half,
            max: center + half,
        }
    }

    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }

    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }

    pub fn center(&self) -> Vec2 {
        (self.min + self.max) / 2.0
    }

    pub fn contains(&self, point: Vec2) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
    }
}

pub struct Camera2D {
    world_bounds: Rect,
    position: Vec2,
    camera_height: f32,
    aspect: f32,
    min_camera_height: f32,
}

impl Camera2D {
    pub fn new(
        world_bounds: Rect,
        focus_bounds: Rect,
        min_camera_height: f32,
        window_aspect: f32,
    ) -> Self {
        let max_h = max_camera_height_for(world_bounds, window_aspect);
        let min_h = min_camera_height.min(max_h);

        let focus_aspect = focus_bounds.width() / focus_bounds.height();
        let camera_height = if focus_aspect > window_aspect {
            focus_bounds.width() / window_aspect
        } else {
            focus_bounds.height()
        }
        .clamp(min_h, max_h);

        let mut cam = Self {
            world_bounds,
            position: focus_bounds.center(),
            camera_height,
            aspect: window_aspect,
            min_camera_height: min_h,
        };
        cam.clamp_position();
        cam
    }

    pub fn camera_height(&self) -> f32 {
        self.camera_height
    }

    pub fn aspect(&self) -> f32 {
        self.aspect
    }

    pub fn camera_rect(&self) -> Rect {
        let half_w = self.camera_height * self.aspect / 2.0;
        let half_h = self.camera_height / 2.0;
        Rect::new(
            self.position - Vec2::new(half_w, half_h),
            self.position + Vec2::new(half_w, half_h),
        )
    }

    pub fn uniforms(&self) -> [f32; 4] {
        let rect = self.camera_rect();
        [rect.min.x, rect.min.y, rect.width(), rect.height()]
    }

    pub fn screen_uniforms(viewport_w: f32, viewport_h: f32) -> [f32; 4] {
        [0.0, 0.0, viewport_w, viewport_h]
    }

    pub fn pan(&mut self, delta: Vec2) {
        self.position += delta;
        self.clamp_position();
    }

    pub fn zoom(&mut self, factor: f32, anchor: Option<Vec2>) {
        let old_height = self.camera_height;
        let max_h = self.max_camera_height();
        self.camera_height = (self.camera_height / factor).clamp(self.min_camera_height, max_h);

        if let Some(anchor) = anchor {
            let scale = self.camera_height / old_height;
            self.position = anchor + (self.position - anchor) * scale;
        }

        self.clamp_position();
    }

    pub fn resize(&mut self, new_aspect: f32) {
        self.aspect = new_aspect;
        let max_h = self.max_camera_height();
        self.camera_height = self.camera_height.clamp(self.min_camera_height, max_h);
        self.clamp_position();
    }

    pub fn screen_to_world(&self, screen_pos: Vec2, viewport_size: Vec2) -> Vec2 {
        let rect = self.camera_rect();
        Vec2::new(
            rect.min.x + (screen_pos.x / viewport_size.x) * rect.width(),
            rect.min.y + (screen_pos.y / viewport_size.y) * rect.height(),
        )
    }

    fn max_camera_height(&self) -> f32 {
        max_camera_height_for(self.world_bounds, self.aspect)
    }

    fn clamp_position(&mut self) {
        let half = Vec2::new(
            self.camera_height * self.aspect / 2.0,
            self.camera_height / 2.0,
        );
        self.position.x = self
            .position
            .x
            .clamp(self.world_bounds.min.x + half.x, self.world_bounds.max.x - half.x);
        self.position.y = self
            .position
            .y
            .clamp(self.world_bounds.min.y + half.y, self.world_bounds.max.y - half.y);
    }
}

fn max_camera_height_for(world_bounds: Rect, aspect: f32) -> f32 {
    let by_width = world_bounds.width() / aspect;
    let by_height = world_bounds.height();
    by_width.min(by_height)
}
