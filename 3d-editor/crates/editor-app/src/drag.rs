use glam::DVec3;

/// The running total of a drag. The editor sends only the step between the last total it applied and the new one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DragOp {
    Translate(DVec3),
    Rotate { axis: DVec3, degrees: f64 },
    Scale(DVec3),
}

impl DragOp {
    pub fn identity(self) -> DragOp {
        match self {
            DragOp::Translate(_) => DragOp::Translate(DVec3::ZERO),
            DragOp::Rotate { axis, .. } => DragOp::Rotate { axis, degrees: 0.0 },
            DragOp::Scale(_) => DragOp::Scale(DVec3::ONE),
        }
    }

    pub fn is_identity(self) -> bool {
        self == self.identity()
    }

    /// The step that takes a drag already applied up to `applied` to `self`.
    pub fn step_from(self, applied: DragOp) -> DragOp {
        match (self, applied) {
            (DragOp::Translate(now), DragOp::Translate(then)) => DragOp::Translate(now - then),
            (DragOp::Rotate { axis, degrees }, DragOp::Rotate { degrees: then, .. }) => {
                DragOp::Rotate {
                    axis,
                    degrees: degrees - then,
                }
            }
            (DragOp::Scale(now), DragOp::Scale(then)) => DragOp::Scale(now / then),
            (now, _) => now,
        }
    }
}

/// A turn between two readings of an angle, taking the short way round the ±180° cut.
pub fn wrap_degrees(step: f64) -> f64 {
    if step > 180.0 {
        step - 360.0
    } else if step < -180.0 {
        step + 360.0
    } else {
        step
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn angle_steps_take_the_short_way_round() {
        assert_eq!(wrap_degrees(350.0), -10.0);
        assert_eq!(wrap_degrees(-350.0), 10.0);
        assert_eq!(wrap_degrees(90.0), 90.0);
    }

    #[test]
    fn translate_steps_are_differences() {
        let step = DragOp::Translate(DVec3::new(16.0, 0.0, 8.0))
            .step_from(DragOp::Translate(DVec3::new(8.0, 0.0, 8.0)));
        assert_eq!(step, DragOp::Translate(DVec3::new(8.0, 0.0, 0.0)));
    }

    #[test]
    fn rotate_steps_subtract_degrees_and_keep_the_axis() {
        let step = DragOp::Rotate {
            axis: DVec3::Z,
            degrees: 45.0,
        }
        .step_from(DragOp::Rotate {
            axis: DVec3::Z,
            degrees: 30.0,
        });
        assert_eq!(
            step,
            DragOp::Rotate {
                axis: DVec3::Z,
                degrees: 15.0
            }
        );
    }

    #[test]
    fn scale_steps_are_ratios() {
        let step = DragOp::Scale(DVec3::new(2.0, 1.0, 1.5))
            .step_from(DragOp::Scale(DVec3::new(1.0, 1.0, 0.5)));
        assert_eq!(step, DragOp::Scale(DVec3::new(2.0, 1.0, 3.0)));
    }

    #[test]
    fn a_step_followed_by_the_inverse_total_is_the_identity() {
        for op in [
            DragOp::Translate(DVec3::new(4.0, -8.0, 0.0)),
            DragOp::Rotate {
                axis: DVec3::X,
                degrees: 90.0,
            },
            DragOp::Scale(DVec3::new(2.0, 0.5, 4.0)),
        ] {
            let undone = op.identity().step_from(op);
            let back = match (op, undone) {
                (DragOp::Translate(a), DragOp::Translate(b)) => (a + b).length() < 1e-12,
                (DragOp::Rotate { degrees: a, .. }, DragOp::Rotate { degrees: b, .. }) => {
                    (a + b).abs() < 1e-12
                }
                (DragOp::Scale(a), DragOp::Scale(b)) => (a * b - DVec3::ONE).length() < 1e-12,
                _ => false,
            };
            assert!(back, "{op:?} -> {undone:?}");
            assert!(op.identity().is_identity());
        }
    }
}
