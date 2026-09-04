use crate::battle::action::SpeedSpec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceStep {
    pub label: String,
    pub speed: SpeedSpec,
    pub dv_penalty: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    pub name: String,
    pub steps: Vec<SequenceStep>,
    pub current: usize,
}

impl Sequence {
    pub fn new(name: impl Into<String>, steps: Vec<SequenceStep>) -> Self {
        Self { name: name.into(), steps, current: 0 }
    }

    /// RULES.md §5.1, pp. 251-253: Shape (N actions, each Speed 5) then Cast (Speed rolled
    /// via Join Battle). The sequence must be unbroken or the spell is lost.
    fn shape_and_cast(circle: &str, shape_actions: usize, shape_dv: i32) -> Self {
        let mut steps: Vec<SequenceStep> = (1..=shape_actions)
            .map(|n| SequenceStep {
                label: format!("Shape {circle} Circle Sorcery ({n}/{shape_actions})"),
                speed: SpeedSpec::Fixed(5),
                dv_penalty: shape_dv,
            })
            .collect();
        steps.push(SequenceStep {
            label: "Cast Sorcery".to_string(),
            speed: SpeedSpec::Variable { default: 5 },
            dv_penalty: 0,
        });
        Self::new(format!("{circle} Circle Sorcery"), steps)
    }

    pub fn shape_terrestrial() -> Self {
        Self::shape_and_cast("Terrestrial", 1, -2)
    }

    pub fn shape_celestial() -> Self {
        Self::shape_and_cast("Celestial", 2, -3)
    }

    pub fn shape_solar() -> Self {
        Self::shape_and_cast("Solar", 3, -4)
    }

    pub fn current_step(&self) -> &SequenceStep {
        &self.steps[self.current]
    }

    pub fn is_final_step(&self) -> bool {
        self.current + 1 == self.steps.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn celestial_sorcery_is_two_shapes_then_a_cast() {
        let sequence = Sequence::shape_celestial();
        assert_eq!(sequence.steps.len(), 3);
        assert_eq!(sequence.steps[0].dv_penalty, -3);
        assert_eq!(sequence.steps[1].dv_penalty, -3);
        assert_eq!(sequence.steps[2].label, "Cast Sorcery");
        assert!(!sequence.is_final_step());
    }

    #[test]
    fn terrestrial_sorcery_is_one_shape_then_a_cast() {
        let sequence = Sequence::shape_terrestrial();
        assert_eq!(sequence.steps.len(), 2);
        assert_eq!(sequence.steps[0].dv_penalty, -2);
    }
}
