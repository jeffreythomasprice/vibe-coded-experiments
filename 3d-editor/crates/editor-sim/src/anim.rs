use glam::DVec3;

use crate::components::{Interpolation, Keyframe, KeyframeAnimation};

/// Keyframes sorted by time, ready to sample.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    keys: Vec<Keyframe>,
    smooth: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub offset: DVec3,
    pub angles: DVec3,
}

impl Track {
    pub fn new(anim: &KeyframeAnimation) -> Self {
        let mut keys = anim.keyframes.clone();
        keys.sort_by(|a, b| a.time.total_cmp(&b.time));
        Self {
            keys,
            smooth: matches!(anim.interpolation, Interpolation::Smooth),
        }
    }

    /// Seconds in one loop: the time of the last keyframe.
    pub fn duration(&self) -> f64 {
        self.keys.last().map_or(0.0, |k| k.time)
    }

    pub fn sample(&self, t: f64) -> Sample {
        let key = |k: &Keyframe| Sample {
            offset: DVec3::from(k.offset),
            angles: DVec3::from(k.angles),
        };
        let Some(first) = self.keys.first() else {
            return Sample {
                offset: DVec3::ZERO,
                angles: DVec3::ZERO,
            };
        };
        let duration = self.duration();
        if self.keys.len() == 1 || duration <= 0.0 || !t.is_finite() {
            return key(first);
        }
        let t = t.rem_euclid(duration);
        if t < first.time {
            return key(first);
        }
        let next = self.keys.partition_point(|k| k.time <= t);
        let (a, b) = match (self.keys.get(next.wrapping_sub(1)), self.keys.get(next)) {
            (Some(a), Some(b)) => (a, b),
            (Some(a), None) => return key(a),
            _ => return key(first),
        };
        let span = b.time - a.time;
        let mut f = if span > 0.0 { (t - a.time) / span } else { 1.0 };
        if self.smooth {
            f = f * f * (3.0 - 2.0 * f);
        }
        let (a, b) = (key(a), key(b));
        Sample {
            offset: a.offset.lerp(b.offset, f),
            angles: a.angles.lerp(b.angles, f),
        }
    }
}
