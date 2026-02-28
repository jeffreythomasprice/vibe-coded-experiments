use web_time::Instant;

pub struct FPSCounter {
    last: Instant,
    samples: [f32; 60],
    head: usize,
    filled: usize,
}

impl FPSCounter {
    pub fn new() -> Self {
        Self {
            last: Instant::now(),
            samples: [0.0; 60],
            head: 0,
            filled: 0,
        }
    }

    pub fn tick(&mut self) -> f32 {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f32();
        self.last = now;

        self.samples[self.head] = dt;
        self.head = (self.head + 1) % 60;
        if self.filled < 60 {
            self.filled += 1;
        }

        let avg_dt = self.samples[..self.filled].iter().sum::<f32>() / self.filled as f32;
        1.0 / avg_dt
    }
}
