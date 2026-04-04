use std::collections::VecDeque;
use std::time::Instant;

pub struct FpsCounter {
    timestamps: VecDeque<Instant>,
}

impl FpsCounter {
    pub fn new() -> Self {
        Self {
            timestamps: VecDeque::new(),
        }
    }

    pub fn tick(&mut self) {
        let now = Instant::now();
        self.timestamps.push_back(now);
        while let Some(&front) = self.timestamps.front() {
            if now.duration_since(front).as_secs_f64() > 1.0 {
                self.timestamps.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn fps(&self) -> f64 {
        if self.timestamps.len() < 2 {
            return 0.0;
        }
        let span = self
            .timestamps
            .back()
            .unwrap()
            .duration_since(*self.timestamps.front().unwrap())
            .as_secs_f64();
        if span == 0.0 {
            return 0.0;
        }
        (self.timestamps.len() - 1) as f64 / span
    }
}
