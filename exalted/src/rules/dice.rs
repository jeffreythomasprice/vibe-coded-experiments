use crate::character::{AbilityKind, AttributeKind, Character};

/// Base dice pool for an Attribute + Ability action, plus an optional
/// specialty (capped at +3 per rulebook). Does not include external
/// modifiers (stunt dice, charms, etc.).
pub fn dice_pool(
    character: &Character,
    attribute: AttributeKind,
    ability: AbilityKind,
    specialty: Option<&str>,
) -> u8 {
    let attr = character.attribute(attribute);
    let abil = character.ability(ability);
    let spec = match specialty {
        None => 0,
        Some(name) => character
            .abilities
            .get(&ability)
            .map(|t| {
                let matching = t.specialties.iter().filter(|s| s.name == name).count();
                if ability == AbilityKind::Linguistics {
                    matching as u8
                } else {
                    matching.min(3) as u8
                }
            })
            .unwrap_or(0),
    };
    attr + abil + spec
}
